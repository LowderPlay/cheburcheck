use anyhow::{Context, Result};
use deadpool::managed::{Manager, Metrics, Object, Pool, RecycleError, RecycleResult};
use klickhouse::{Client, ClientOptions, KlickhouseError, QueryBuilder, Row};
use rocket::tokio::sync::Semaphore;
use rocket::tokio::time::timeout;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

mod embedded {
    refinery::embed_migrations!("clickhouse/migrations");
}

const COMPLAINTS_SQL: &str = include_str!("../clickhouse/queries/complaints.sql");
const QUERY_SQL: &str = include_str!("../clickhouse/queries/query_by_id.sql");
pub const INSERT_QUERY_SQL: &str = include_str!("../clickhouse/queries/insert_query.sql");
pub const INSERT_PROBE_REPORT_SQL: &str =
    include_str!("../clickhouse/queries/insert_probe_report.sql");

struct ConnectionConfig {
    host: String,
    port: u16,
    options: ClientOptions,
    stats_timezone: String,
}

impl ConnectionConfig {
    fn parse(uri: &str) -> Result<Self> {
        let uri = url::Url::parse(uri).context("invalid ClickHouse URI")?;
        anyhow::ensure!(
            uri.scheme() == "clickhouse",
            "ClickHouse URI must use clickhouse:// for native TCP"
        );
        anyhow::ensure!(
            uri.fragment().is_none(),
            "ClickHouse URI must not contain a fragment"
        );
        let host = match uri.host().context("ClickHouse URI must include a host")? {
            url::Host::Domain(host) => host.to_owned(),
            url::Host::Ipv4(host) => host.to_string(),
            url::Host::Ipv6(host) => host.to_string(),
        };
        let database = uri.path().strip_prefix('/').unwrap_or(uri.path());
        anyhow::ensure!(
            !database.is_empty() && !database.contains('/'),
            "ClickHouse URI must include one database path segment"
        );
        let decode = |value: &str| -> Result<String> {
            Ok(percent_encoding::percent_decode_str(value)
                .decode_utf8()
                .context("ClickHouse URI contains invalid UTF-8")?
                .into_owned())
        };
        let mut stats_timezone = None;
        for (name, value) in uri.query_pairs() {
            anyhow::ensure!(
                name == "stats_timezone",
                "unsupported ClickHouse URI query parameter"
            );
            anyhow::ensure!(
                stats_timezone.is_none() && !value.is_empty(),
                "specify stats_timezone once with a nonempty value"
            );
            stats_timezone = Some(value.into_owned());
        }
        Ok(Self {
            host,
            port: uri.port().unwrap_or(9000),
            options: ClientOptions {
                username: if uri.username().is_empty() {
                    "default".into()
                } else {
                    decode(uri.username())?
                },
                password: decode(uri.password().unwrap_or_default())?,
                default_database: decode(database)?,
                ..Default::default()
            },
            stats_timezone: stats_timezone.unwrap_or_else(|| "UTC".into()),
        })
    }

    async fn connect(&self) -> Result<(Connection, String)> {
        let connection = timeout(Duration::from_secs(10), async {
            let stream =
                rocket::tokio::net::TcpStream::connect((self.host.as_str(), self.port)).await?;
            stream.set_nodelay(self.options.tcp_nodelay)?;
            let stream = stream.into_std()?;
            // Retain a shutdown handle before starting the driver's background task.
            // Its initial handshake does not observe dropped Client handles.
            let socket = SocketGuard(stream.try_clone()?);
            let stream = rocket::tokio::net::TcpStream::from_std(stream)?;
            let (reader, writer) = stream.into_split();
            let client = Client::connect_stream(reader, writer, self.options.clone()).await?;
            anyhow::ensure!(
                !client.is_closed(),
                "ClickHouse connection closed during setup"
            );
            Ok::<_, anyhow::Error>(Connection {
                client,
                _socket: socket,
            })
        })
        .await
        .context("ClickHouse connection timed out")?
        .context("failed to connect to ClickHouse")?;
        Ok((connection, self.stats_timezone.clone()))
    }
}

struct SocketGuard(std::net::TcpStream);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

struct Connection {
    client: Client,
    _socket: SocketGuard,
}

impl std::ops::Deref for Connection {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

#[derive(Clone, Copy)]
struct PoolSettings {
    size: usize,
    acquire_timeout: Duration,
    query_timeout: Duration,
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            size: 4,
            acquire_timeout: Duration::from_secs(5),
            query_timeout: Duration::from_secs(10),
        }
    }
}

impl PoolSettings {
    fn from_env() -> Result<Self> {
        fn positive(name: &str, default: usize) -> Result<usize> {
            match std::env::var(name) {
                Ok(value) => {
                    let value = value
                        .parse::<usize>()
                        .with_context(|| format!("invalid {name}"))?;
                    anyhow::ensure!(value > 0, "{name} must be positive");
                    Ok(value)
                }
                Err(std::env::VarError::NotPresent) => Ok(default),
                Err(error) => Err(error).with_context(|| format!("invalid {name}")),
            }
        }
        let defaults = Self::default();
        let size = positive("CLICKHOUSE_POOL_SIZE", defaults.size)?;
        anyhow::ensure!(
            size <= Semaphore::MAX_PERMITS,
            "CLICKHOUSE_POOL_SIZE is too large"
        );
        Ok(Self {
            size,
            acquire_timeout: Duration::from_secs(
                positive("CLICKHOUSE_ACQUIRE_TIMEOUT_SECONDS", 5)? as u64,
            ),
            query_timeout: Duration::from_secs(
                positive("CLICKHOUSE_QUERY_TIMEOUT_SECONDS", 10)? as u64
            ),
        })
    }
}

struct ConnectionManager {
    config: ConnectionConfig,
}

impl Manager for ConnectionManager {
    type Type = Connection;
    type Error = KlickhouseError;

    async fn create(&self) -> klickhouse::Result<Connection> {
        self.config
            .connect()
            .await
            .map(|(connection, _)| connection)
            .map_err(|error| {
                KlickhouseError::ProtocolError(format!(
                    "failed to connect to ClickHouse: {error:#}"
                ))
            })
    }

    async fn recycle(
        &self,
        connection: &mut Connection,
        _: &Metrics,
    ) -> RecycleResult<KlickhouseError> {
        if connection.is_closed() {
            Err(RecycleError::message("ClickHouse connection closed"))
        } else {
            Ok(())
        }
    }
}

struct ConnectionPool {
    inner: Pool<ConnectionManager>,
    settings: PoolSettings,
}

// A lease starts non-reusable: cancellation or a timeout must close the connection,
// since the driver's query may still be running. Only a completed operation can
// return it to the pool. The socket guard explicitly closes discarded connections.
struct ConnectionLease {
    client: Option<Object<ConnectionManager>>,
    reusable: bool,
}

impl ConnectionLease {
    fn client(&self) -> &Client {
        self.client.as_ref().expect("connection lease has a client")
    }

    fn finish<T>(&mut self, result: klickhouse::Result<T>) -> klickhouse::Result<T> {
        // A decoding error can leave unread blocks in flight. Only success or a
        // server exception establishes that the operation has finished safely.
        self.reusable = !self.client().is_closed()
            && matches!(
                &result,
                Ok(_) | Err(KlickhouseError::ServerException { .. })
            );
        result.and_then(|value| {
            if self.client().is_closed() {
                Err(KlickhouseError::ProtocolError(
                    "ClickHouse connection closed while executing query".into(),
                ))
            } else {
                Ok(value)
            }
        })
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        if !self.reusable {
            if let Some(connection) = self.client.take() {
                // Remove it permanently: returning a timed-out operation to the
                // pool would queue later requests behind the unfinished query.
                drop(Object::take(connection));
            }
        }
    }
}

impl ConnectionPool {
    async fn acquire(&self) -> klickhouse::Result<ConnectionLease> {
        let client = timeout(self.settings.acquire_timeout, self.inner.get())
            .await
            .map_err(|_| {
                KlickhouseError::ProtocolError("ClickHouse pool acquisition timed out".into())
            })?
            .map_err(|error| KlickhouseError::ProtocolError(format!("ClickHouse pool: {error}")))?;
        Ok(ConnectionLease {
            client: Some(client),
            reusable: false,
        })
    }
}

#[derive(Clone)]
pub struct Analytics {
    pool: Arc<ConnectionPool>,
    stats_timezone: String,
}

#[derive(klickhouse::Row)]
pub struct StoredQuery {
    pub query: String,
    pub resolved_ips: Vec<Option<String>>,
}

#[derive(klickhouse::Row, serde::Serialize, Debug)]
pub struct ComplaintDay {
    pub date: String,
    pub count: i64,
}

impl Analytics {
    pub async fn connect_from_env() -> Result<Self> {
        let uri = std::env::var("CLICKHOUSE_URL")
            .context("CLICKHOUSE_URL must specify clickhouse://user:password@host:9000/database")?;
        let config = ConnectionConfig::parse(&uri)?;
        let settings = PoolSettings::from_env()?;
        let analytics = Self::new(config, settings)?;
        analytics
            .execute(
                QueryBuilder::new("SELECT toDate(now(), $1)").arg(analytics.stats_timezone.clone()),
            )
            .await
            .context("failed to validate ClickHouse stats_timezone")?;
        Ok(analytics)
    }

    fn new(config: ConnectionConfig, settings: PoolSettings) -> Result<Self> {
        let stats_timezone = config.stats_timezone.clone();
        let inner = Pool::builder(ConnectionManager { config })
            .max_size(settings.size)
            .build()
            .context("failed to build ClickHouse pool")?;
        Ok(Self {
            pool: Arc::new(ConnectionPool { inner, settings }),
            stats_timezone,
        })
    }

    pub async fn migrate(&self) -> Result<()> {
        let mut connection = self.pool.acquire().await?;
        // Migrations may contain multiple DDL statements and need a longer deadline.
        let result = timeout(
            Duration::from_secs(120),
            embedded::migrations::runner()
                .run_async(&mut connection.client.as_mut().unwrap().client),
        )
        .await
        .context("ClickHouse migrations timed out")?;
        connection.reusable = result.is_ok() && !connection.client().is_closed();
        result.context("failed to apply embedded ClickHouse migrations")?;
        anyhow::ensure!(
            connection.reusable,
            "ClickHouse connection closed during migrations"
        );
        Ok(())
    }

    async fn read<T: Row>(&self, query: QueryBuilder<'_>) -> klickhouse::Result<Vec<T>> {
        let query = query.finalize()?.to_string();
        for attempt in 0..2 {
            let mut connection = self.pool.acquire().await?;
            let result = timeout(
                self.pool.settings.query_timeout,
                connection.client().query_collect(query.clone()),
            )
            .await
            .map_err(|_| KlickhouseError::ProtocolError("ClickHouse query timed out".into()))?;
            let result = connection.finish(result);
            if attempt == 0
                && matches!(
                    &result,
                    Err(KlickhouseError::Io(_) | KlickhouseError::ProtocolError(_))
                )
            {
                // Release the failed lease before acquiring a replacement (also for size 1).
                drop(connection);
                continue;
            }
            return result;
        }
        unreachable!("read makes at most two attempts")
    }

    // A failed insert can already have committed. Discard its connection without
    // automatically replaying an ambiguous write.
    pub async fn execute(&self, query: QueryBuilder<'_>) -> klickhouse::Result<()> {
        let mut connection = self.pool.acquire().await?;
        let result = timeout(
            self.pool.settings.query_timeout,
            connection.client().execute(query),
        )
        .await
        .map_err(|_| KlickhouseError::ProtocolError("ClickHouse query timed out".into()))?;
        connection.finish(result)
    }

    pub async fn query_by_id(&self, id: Uuid) -> klickhouse::Result<Option<StoredQuery>> {
        Ok(self.read(QueryBuilder::new(QUERY_SQL).arg(id)).await?.pop())
    }

    pub async fn complaints(&self, target: &str) -> klickhouse::Result<Vec<ComplaintDay>> {
        self.read(
            QueryBuilder::new(COMPLAINTS_SQL)
                .arg(target)
                .arg(self.stats_timezone.clone()),
        )
        .await
    }

    pub async fn feedback(
        &self,
        id: Uuid,
        query: &str,
        source_ip: &str,
        works: bool,
    ) -> klickhouse::Result<()> {
        self.execute(
                QueryBuilder::new(
                    "INSERT INTO human_reports (query_id, query, source_ip, works) VALUES ($1, $2, $3, $4)",
                )
                .arg(id)
                .arg(query)
                .arg(source_ip)
                .arg(works),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clickhouse_uri_defaults() {
        let config = ConnectionConfig::parse("clickhouse://localhost/analytics").unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 9000);
        assert_eq!(config.options.username, "default");
        assert_eq!(config.options.password, "");
        assert_eq!(config.options.default_database, "analytics");
        assert_eq!(config.stats_timezone, "UTC");
    }

    #[test]
    fn parses_encoded_credentials_ipv6_and_timezone() {
        let config = ConnectionConfig::parse(
            "clickhouse://user%40team:p%40ss%3A%2F%23%25@[::1]:19000/analytics%20db?stats_timezone=Asia%2FYekaterinburg",
        ).unwrap();
        assert_eq!(config.host, "::1");
        assert_eq!(config.port, 19000);
        assert_eq!(config.options.username, "user@team");
        assert_eq!(config.options.password, "p@ss:/#%");
        assert_eq!(config.options.default_database, "analytics db");
        assert_eq!(config.stats_timezone, "Asia/Yekaterinburg");
    }

    #[test]
    fn rejects_invalid_clickhouse_uris_without_disclosing_credentials() {
        for uri in [
            "clickhouse://user:secret@localhost",
            "clickhouse://user:secret@localhost/a/b",
            "http://user:secret@localhost/analytics",
            "clickhouses://user:secret@localhost/analytics",
            "clickhouse://user:secret@localhost:bad/analytics",
            "clickhouse://user:secret@localhost/analytics#fragment",
            "clickhouse://user:secret@localhost/analytics?unknown=value",
            "clickhouse://user:secret@localhost/analytics?stats_timezone=",
            "clickhouse://user:secret@localhost/analytics?stats_timezone=UTC&stats_timezone=UTC",
            "clickhouse://user:secret@localhost/%FF",
        ] {
            let error = ConnectionConfig::parse(uri)
                .err()
                .expect("URI must be rejected");
            assert!(!format!("{error:#}").contains("secret"));
        }
    }

    #[derive(klickhouse::Row, Debug)]
    struct Count {
        count: u64,
    }

    #[derive(klickhouse::Row)]
    struct StoredProbe {
        query_id: Uuid,
        result: String,
        verdicts: Vec<Option<String>>,
        duration_ms: Option<i64>,
    }

    async fn test_analytics(settings: PoolSettings) -> Result<Analytics> {
        let config = ConnectionConfig::parse(&std::env::var("CLICKHOUSE_TEST_URL")?)?;
        let analytics = Analytics::new(config, settings)?;
        analytics.execute(QueryBuilder::new("SELECT 1")).await?;
        Ok(analytics)
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn pool_is_bounded_and_shared_between_clones() -> Result<()> {
        let analytics = test_analytics(PoolSettings {
            size: 2,
            acquire_timeout: Duration::from_millis(100),
            ..PoolSettings::default()
        })
        .await?;
        let clone = analytics.clone();
        let mut first = analytics.pool.acquire().await?;
        let mut second = clone.pool.acquire().await?;
        assert_eq!(analytics.pool.inner.status().available, 0);
        assert!(analytics.pool.acquire().await.is_err());
        assert_eq!(analytics.pool.inner.status().available, 0);

        // Separate native connections have distinct TCP source addresses.
        let first_address = first.client.as_ref().unwrap()._socket.0.local_addr()?;
        let second_address = second.client.as_ref().unwrap()._socket.0.local_addr()?;
        assert_ne!(first_address, second_address);
        let result = first.client().execute("SELECT 1").await;
        first.finish(result)?;
        let result = second.client().execute("SELECT 1").await;
        second.finish(result)?;
        drop(first);
        let mut reused = clone.pool.acquire().await?;
        let reused_address = reused.client.as_ref().unwrap()._socket.0.local_addr()?;
        assert_eq!(first_address, reused_address);
        let result = reused.client().execute("SELECT 1").await;
        reused.finish(result)?;
        drop(reused);
        drop(second);
        assert_eq!(analytics.pool.inner.status().available, 2);
        assert_eq!(analytics.pool.inner.status().size, 2);
        Ok(())
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn slow_query_does_not_block_other_connections() -> Result<()> {
        let analytics = test_analytics(PoolSettings {
            size: 2,
            ..PoolSettings::default()
        })
        .await?;
        let mut slow = analytics.pool.acquire().await?;
        let (slow_result, fast_result) = rocket::tokio::join!(
            slow.client().execute("SELECT sleep(1)"),
            timeout(
                Duration::from_millis(500),
                analytics.read::<Count>(QueryBuilder::new("SELECT toUInt64(1) AS count"))
            ),
        );
        slow.finish(slow_result)?;
        assert_eq!(fast_result??[0].count, 1);
        Ok(())
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn query_timeouts_discard_connections_and_release_capacity() -> Result<()> {
        let analytics = test_analytics(PoolSettings {
            size: 1,
            query_timeout: Duration::from_millis(100),
            ..PoolSettings::default()
        })
        .await?;
        assert!(
            analytics
                .execute(QueryBuilder::new("SELECT sleep(1)"))
                .await
                .unwrap_err()
                .to_string()
                .contains("query timed out")
        );
        assert_eq!(analytics.pool.inner.status().available, 0);
        assert_eq!(analytics.pool.inner.status().size, 0);
        // Recovery must use a new connection rather than queue behind the slow query.
        let rows: Vec<Count> = analytics
            .read(QueryBuilder::new("SELECT toUInt64(1) AS count"))
            .await?;
        assert_eq!(rows[0].count, 1);
        assert!(
            analytics
                .read::<Count>(QueryBuilder::new(
                    "SELECT toUInt64(1) AS count WHERE sleep(1) = 0"
                ))
                .await
                .unwrap_err()
                .to_string()
                .contains("query timed out")
        );
        assert_eq!(analytics.pool.inner.status().available, 0);
        assert_eq!(analytics.pool.inner.status().size, 0);
        analytics.execute(QueryBuilder::new("SELECT 1")).await?;
        Ok(())
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn cancellation_discards_connection_and_releases_capacity() -> Result<()> {
        let analytics = test_analytics(PoolSettings {
            size: 1,
            ..PoolSettings::default()
        })
        .await?;
        // Cancelling a future with a lease must not return an in-flight query to the pool.
        assert!(
            timeout(
                Duration::from_millis(100),
                analytics.execute(QueryBuilder::new("SELECT sleep(1)")),
            )
            .await
            .is_err()
        );
        assert_eq!(analytics.pool.inner.status().available, 0);
        assert_eq!(analytics.pool.inner.status().size, 0);
        let rows: Vec<Count> = analytics
            .read(QueryBuilder::new("SELECT toUInt64(1) AS count"))
            .await?;
        assert_eq!(rows[0].count, 1);
        Ok(())
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn stalled_handshake_times_out_and_closes_socket() -> Result<()> {
        use rocket::tokio::{io::AsyncReadExt, net::TcpListener};
        let mut config = ConnectionConfig::parse(&std::env::var("CLICKHOUSE_TEST_URL")?)?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        config.host = address.ip().to_string();
        config.port = address.port();
        let analytics = Analytics::new(
            config,
            PoolSettings {
                size: 1,
                acquire_timeout: Duration::from_millis(100),
                ..PoolSettings::default()
            },
        )?;
        let peer = rocket::tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await?;
            // Accept TCP but never send the ClickHouse hello. Cancellation must
            // close the socket even though the driver's handshake is still waiting.
            let mut received = Vec::new();
            socket.read_to_end(&mut received).await?;
            Ok::<_, std::io::Error>(received)
        });
        let error = analytics
            .pool
            .acquire()
            .await
            .err()
            .expect("handshake must time out");
        assert!(error.to_string().contains("acquisition timed out"));
        assert_eq!(analytics.pool.inner.status().size, 0);
        assert!(!timeout(Duration::from_secs(2), peer).await???.is_empty());
        Ok(())
    }

    // A single-connection TCP proxy lets the test simulate a server outage
    // without stopping or modifying the disposable ClickHouse server itself.
    fn proxy(
        listener: rocket::tokio::net::TcpListener,
        upstream: (String, u16),
        drop_read_once: bool,
    ) -> rocket::tokio::task::JoinHandle<()> {
        rocket::tokio::spawn(async move {
            if drop_read_once {
                use rocket::tokio::io::{AsyncReadExt, AsyncWriteExt};
                let (mut downstream, _) = listener.accept().await.unwrap();
                let mut upstream_socket = rocket::tokio::net::TcpStream::connect(upstream.clone())
                    .await
                    .unwrap();
                let mut received = Vec::new();
                let mut down_buffer = [0; 4096];
                let mut up_buffer = [0; 4096];
                loop {
                    rocket::tokio::select! {
                        bytes = downstream.read(&mut down_buffer) => {
                            let bytes = bytes.unwrap();
                            if bytes == 0 { break; }
                            received.extend_from_slice(&down_buffer[..bytes]);
                            if received.windows(b"disconnect_test_marker".len())
                                .any(|part| part == b"disconnect_test_marker") {
                                break;
                            }
                            upstream_socket.write_all(&down_buffer[..bytes]).await.unwrap();
                        }
                        bytes = upstream_socket.read(&mut up_buffer) => {
                            let bytes = bytes.unwrap();
                            if bytes == 0 { break; }
                            downstream.write_all(&up_buffer[..bytes]).await.unwrap();
                        }
                    }
                }
                // Drop only the first connection; keep listening for the read retry.
            }
            let (mut downstream, _) = listener.accept().await.unwrap();
            let mut upstream = rocket::tokio::net::TcpStream::connect(upstream)
                .await
                .unwrap();
            let _ = rocket::tokio::io::copy_bidirectional(&mut downstream, &mut upstream).await;
        })
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn reconnects_after_outage() -> Result<()> {
        use rocket::tokio::{
            net::TcpListener,
            time::{Duration, timeout},
        };
        let uri = std::env::var("CLICKHOUSE_TEST_URL")?;
        let mut config = ConnectionConfig::parse(&uri)?;
        let upstream = (config.host.clone(), config.port);
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let forwarding = proxy(listener, upstream.clone(), false);
        config.host = address.ip().to_string();
        config.port = address.port();
        let analytics = Analytics::new(
            config,
            PoolSettings {
                size: 1,
                ..PoolSettings::default()
            },
        )?;
        let clone = analytics.clone();

        let rows: Vec<Count> = analytics
            .read(QueryBuilder::new("SELECT toUInt64(1) AS count"))
            .await?;
        assert_eq!(rows[0].count, 1);

        let mut lease = analytics.pool.acquire().await?;
        let initial = lease.client().clone();
        lease.finish(Ok(()))?;
        drop(lease);
        forwarding.abort();
        let _ = forwarding.await;
        timeout(Duration::from_secs(2), async {
            while !initial.is_closed() {
                rocket::tokio::task::yield_now().await;
            }
        })
        .await?;
        // While unavailable, requests fail rather than retaining a broken client forever.
        assert!(
            clone
                .read::<Count>(QueryBuilder::new("SELECT toUInt64(1) AS count"))
                .await
                .is_err()
        );

        let listener = TcpListener::bind(address).await?;
        let forwarding = proxy(listener, upstream, true);
        let rows: Vec<Count> = analytics
            .read(QueryBuilder::new(
                "SELECT toUInt64(2) AS count /* disconnect_test_marker */",
            ))
            .await?;
        assert_eq!(rows[0].count, 2);
        clone.execute(QueryBuilder::new("SELECT 1")).await?;
        forwarding.abort();
        let _ = forwarding.await;
        Ok(())
    }

    #[rocket::async_test]
    #[ignore = "requires CLICKHOUSE_TEST_URL pointing to a disposable native ClickHouse server"]
    async fn migrations_and_analytics_queries() -> Result<()> {
        let uri = std::env::var("CLICKHOUSE_TEST_URL")?;
        let mut config = ConnectionConfig::parse(&uri)?;
        let (client, _) = config.connect().await?;
        let database = format!("blocklist_test_{}", Uuid::new_v4().simple());
        client
            .execute(format!("CREATE DATABASE {database}"))
            .await?;
        client.execute(format!("USE {database}")).await?;
        config.options.default_database = database.clone();
        let analytics = Analytics::new(config, PoolSettings::default())?;
        let result: Result<()> = async {
            analytics.migrate().await?;
            analytics.migrate().await?;
            let applied = client
                .query_one::<Count>("SELECT count() AS count FROM refinery_schema_history")
                .await?;
            assert_eq!(applied.count, 1);
            let missing = Uuid::new_v4();
            assert!(analytics.query_by_id(missing).await?.is_none());
            let id = Uuid::new_v4();
            // Quotes, backslashes, Unicode and dollar placeholders must remain data.
            let target = "Example.org'\\пример$1";
            analytics
                .execute(
                    QueryBuilder::new(INSERT_QUERY_SQL)
                        .arg(id)
                        .arg(target)
                        .arg("2001:db8::1")
                        .arg(None::<String>)
                        .arg(None::<i32>)
                        .arg(None::<String>)
                        .arg(None::<String>)
                        .arg(None::<String>)
                        .arg(vec![Some("1.2.3.4".to_owned()), None])
                        .arg(Vec::<String>::new())
                        .arg(Vec::<String>::new())
                        .arg(None::<String>),
                )
                .await?;
            let stored = analytics.query_by_id(id).await?.unwrap();
            assert_eq!(stored.query, target);
            assert_eq!(stored.resolved_ips, vec![Some("1.2.3.4".into()), None]);
            let empty = analytics.complaints(target).await?;
            assert_eq!(empty.len(), 14);
            assert!(empty.iter().all(|day| day.count == 0));
            analytics.feedback(id, target, "1.2.3.4", false).await?;
            analytics.feedback(id, target, "1.2.3.4", false).await?;
            analytics.feedback(id, target, "1.2.3.5", false).await?;
            analytics.feedback(id, target, "1.2.3.6", true).await?;
            analytics
                .feedback(id, "other.org", "1.2.3.7", false)
                .await?;
            let total = client
                .query_one::<Count>("SELECT count() AS count FROM human_reports")
                .await?;
            assert_eq!(total.count, 5);
            let days = analytics.complaints(&target.to_lowercase()).await?;
            assert_eq!(days.len(), 14);
            assert_eq!(days.last().unwrap().count, 2);
            assert_eq!(days.iter().map(|day| day.count).sum::<i64>(), 2);
            assert!(days.windows(2).all(|pair| pair[0].date < pair[1].date));
            let json = r#"{"message":"quoted \"value\"\nпример"}"#;
            analytics
                .execute(
                    QueryBuilder::new(INSERT_PROBE_REPORT_SQL)
                        .arg(id)
                        .arg(target)
                        .arg(1_i32)
                        .arg(vec!["blocked".to_owned()])
                        .arg(json)
                        .arg(None::<i16>)
                        .arg(None::<String>)
                        .arg(1234_i64),
                )
                .await?;
            let probe = client
                .query_one::<StoredProbe>(
                    "SELECT query_id, result, verdicts, duration_ms FROM probe_reports",
                )
                .await?;
            assert_eq!(probe.query_id, id);
            assert_eq!(probe.result, json);
            assert_eq!(probe.verdicts, vec![Some("blocked".into())]);
            assert_eq!(probe.duration_ms, Some(1234));
            let available_before = analytics.pool.inner.status().available;
            assert!(
                analytics
                    .execute(QueryBuilder::new("SELECT unknown_column"))
                    .await
                    .is_err()
            );
            assert_eq!(available_before, analytics.pool.inner.status().available);
            Ok(())
        }
        .await;
        client.execute("USE default").await?;
        client
            .execute(format!("DROP DATABASE {database} SYNC"))
            .await?;
        result
    }
}
