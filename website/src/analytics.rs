use anyhow::{Context, Result};
use klickhouse::{Client, ClientOptions, KlickhouseError, QueryBuilder, Row};
use rocket::tokio::sync::Mutex;
use std::sync::Arc;
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

    async fn connect(&self) -> Result<(Client, String)> {
        let client = rocket::tokio::time::timeout(
            std::time::Duration::from_secs(10),
            Client::connect((self.host.as_str(), self.port), self.options.clone()),
        )
        .await
        .context("ClickHouse connection timed out")?
        .context("failed to connect to ClickHouse")?;
        anyhow::ensure!(
            !client.is_closed(),
            "ClickHouse connection closed during setup"
        );
        Ok((client, self.stats_timezone.clone()))
    }
}

#[derive(Clone)]
pub struct Analytics {
    connection: Arc<Mutex<Option<Arc<Client>>>>,
    config: Arc<ConnectionConfig>,
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
        let (client, stats_timezone) = config.connect().await?;
        client
            .execute(QueryBuilder::new("SELECT toDate(now(), $1)").arg(stats_timezone.clone()))
            .await
            .context("invalid ClickHouse stats_timezone")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(Some(Arc::new(client)))),
            config: Arc::new(config),
            stats_timezone,
        })
    }

    pub async fn migrate(&self) -> Result<()> {
        embedded::migrations::runner()
            .run_async(&mut self.client().await?.as_ref().clone())
            .await
            .context("failed to apply embedded ClickHouse migrations")?;
        Ok(())
    }

    // All Analytics clones share the replacement connection. Serialize reconnects,
    // but release the lock before running queries on the multiplexed client.
    async fn client(&self) -> klickhouse::Result<Arc<Client>> {
        let mut connection = self.connection.lock().await;
        if let Some(client) = connection.as_ref().filter(|client| !client.is_closed()) {
            return Ok(client.clone());
        }
        let (client, _) = self.config.connect().await.map_err(|error| {
            KlickhouseError::ProtocolError(format!("failed to reconnect to ClickHouse: {error:#}"))
        })?;
        let client = Arc::new(client);
        *connection = Some(client.clone());
        log::info!("reconnected to ClickHouse");
        Ok(client)
    }

    async fn invalidate(&self, failed: &Arc<Client>, error: &KlickhouseError) -> bool {
        let disconnected = failed.is_closed()
            || matches!(
                error,
                KlickhouseError::Io(_) | KlickhouseError::ProtocolError(_)
            );
        if disconnected {
            let mut connection = self.connection.lock().await;
            // An older in-flight request must not discard a newer connection.
            if connection
                .as_ref()
                .is_some_and(|client| Arc::ptr_eq(client, failed))
            {
                *connection = None;
            }
        }
        disconnected
    }

    fn check_result<T>(client: &Client, result: klickhouse::Result<T>) -> klickhouse::Result<T> {
        // Klickhouse can end a result stream without emitting an error when its
        // connection task exits. Do not accept partial reads or ambiguous writes
        // as successful just because the stream ended.
        result.and_then(|value| {
            if client.is_closed() {
                Err(KlickhouseError::ProtocolError(
                    "ClickHouse connection closed while executing query".into(),
                ))
            } else {
                Ok(value)
            }
        })
    }

    async fn read<T: Row>(&self, query: QueryBuilder<'_>) -> klickhouse::Result<Vec<T>> {
        let query = query.finalize()?.to_string();
        let client = self.client().await?;
        match Self::check_result(&client, client.query_collect(query.clone()).await) {
            Ok(rows) => Ok(rows),
            Err(error) => {
                if !self.invalidate(&client, &error).await {
                    return Err(error);
                }
                let client = self.client().await?;
                let result = Self::check_result(&client, client.query_collect(query).await);
                if let Err(error) = &result {
                    self.invalidate(&client, error).await;
                }
                result
            }
        }
    }

    // A failed insert can already have committed. Reconnect for subsequent
    // requests without automatically replaying an ambiguous write.
    pub async fn execute(&self, query: QueryBuilder<'_>) -> klickhouse::Result<()> {
        let client = self.client().await?;
        let result = Self::check_result(&client, client.execute(query).await);
        if let Err(error) = &result {
            self.invalidate(&client, error).await;
        }
        result
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

    #[derive(klickhouse::Row)]
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
        let (client, stats_timezone) = config.connect().await?;
        let analytics = Analytics {
            connection: Arc::new(Mutex::new(Some(Arc::new(client)))),
            config: Arc::new(config),
            stats_timezone,
        };
        let clone = analytics.clone();
        let initial = analytics.client().await?;
        let rows: Vec<Count> = analytics
            .read(QueryBuilder::new("SELECT toUInt64(1) AS count"))
            .await?;
        assert_eq!(rows[0].count, 1);

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
        // Concurrent callers all obtain the same replacement connection.
        let (first, second) = rocket::tokio::join!(analytics.client(), clone.client());
        let first = first?;
        assert!(Arc::ptr_eq(&first, &second?));
        assert!(!Arc::ptr_eq(&initial, &first));
        // An older failed request cannot invalidate a successfully reconnected client.
        analytics
            .invalidate(&initial, &KlickhouseError::ProtocolError("closed".into()))
            .await;
        assert!(Arc::ptr_eq(&first, &analytics.client().await?));
        clone.execute(QueryBuilder::new("SELECT 1")).await?;
        let rows: Vec<Count> = analytics
            .read(QueryBuilder::new(
                "SELECT toUInt64(2) AS count /* disconnect_test_marker */",
            ))
            .await?;
        assert_eq!(rows[0].count, 2);
        assert!(!Arc::ptr_eq(&first, &analytics.client().await?));
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
        let analytics = Analytics {
            connection: Arc::new(Mutex::new(Some(Arc::new(client.clone())))),
            config: Arc::new(config),
            stats_timezone: "UTC".into(),
        };
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
            let before = analytics.client().await?;
            assert!(
                analytics
                    .execute(QueryBuilder::new("SELECT unknown_column"))
                    .await
                    .is_err()
            );
            assert!(Arc::ptr_eq(&before, &analytics.client().await?));
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
