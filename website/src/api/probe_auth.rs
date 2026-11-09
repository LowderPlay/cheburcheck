use rocket::Request;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashSet;
use std::time::Duration;

pub struct ProbeCredentials<'r>(pub Option<&'r str>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ProbeCredentials<'r> {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let mut headers = request.headers().get("Authorization");
        let Some(header) = headers.next() else {
            return Outcome::Success(Self(None));
        };
        if headers.next().is_some() {
            return Outcome::Error((Status::Unauthorized, ()));
        }
        match header.split_once(' ') {
            Some((scheme, key))
                if scheme.eq_ignore_ascii_case("Bearer")
                    && !key.is_empty()
                    && key.len() <= 512
                    && !key.chars().any(char::is_whitespace) =>
            {
                Outcome::Success(Self(Some(key)))
            }
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

pub struct ProbeAuth {
    client: reqwest::Client,
    secret: Option<String>,
    hostnames: HashSet<String>,
    endpoint: String,
}

#[derive(Deserialize)]
struct Verification {
    success: bool,
    action: Option<String>,
    hostname: Option<String>,
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<VerificationErrorCode>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum VerificationErrorCode {
    MissingInputSecret,
    InvalidInputSecret,
    MissingInputResponse,
    InvalidInputResponse,
    BadRequest,
    TimeoutOrDuplicate,
    InternalError,
    #[serde(other)]
    Unknown,
}

impl ProbeAuth {
    pub fn from_env() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build Turnstile client"),
            secret: std::env::var("TURNSTILE_SECRET")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            hostnames: std::env::var("TURNSTILE_HOSTNAMES")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect(),
            endpoint: "https://challenges.cloudflare.com/turnstile/v0/siteverify".into(),
        }
    }

    pub async fn authorize(
        &self,
        pool: &PgPool,
        credentials: ProbeCredentials<'_>,
        token: Option<&str>,
    ) -> Result<Option<uuid::Uuid>, Status> {
        if let Some(key) = credentials.0 {
            let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
            let id = sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT id FROM probe_api_keys WHERE key_hash = $1
                 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())",
            )
            .bind(hash)
            .fetch_optional(pool)
            .await
            .map_err(|error| {
                log::warn!("api: failed to verify probe API key: {error}");
                Status::ServiceUnavailable
            })?;
            return id.map(Some).ok_or(Status::Unauthorized);
        }
        self.verify_turnstile(token).await.map(|()| None)
    }

    async fn verify_turnstile(&self, token: Option<&str>) -> Result<(), Status> {
        let token = token
            .filter(|value| !value.is_empty() && value.len() <= 2048)
            .ok_or(Status::Forbidden)?;
        let secret = self.secret.as_deref().ok_or(Status::ServiceUnavailable)?;
        if self.hostnames.is_empty() {
            return Err(Status::ServiceUnavailable);
        }
        // Cloudflare consumes the token once; never retry siteverify automatically.
        let response = self
            .client
            .post(&self.endpoint)
            .form(&[("secret", secret), ("response", token)])
            .send()
            .await
            .map_err(|_| Status::ServiceUnavailable)?;
        if !response.status().is_success() {
            return Err(Status::ServiceUnavailable);
        }
        let result: Verification = response
            .json()
            .await
            .map_err(|_| Status::ServiceUnavailable)?;
        if !result.success {
            let unavailable = result.error_codes.iter().any(|code| {
                matches!(
                    code,
                    VerificationErrorCode::MissingInputSecret
                        | VerificationErrorCode::InvalidInputSecret
                        | VerificationErrorCode::InternalError
                )
            });
            return Err(if unavailable {
                Status::ServiceUnavailable
            } else {
                Status::Forbidden
            });
        }
        if result.action.as_deref() != Some("probe") {
            return Err(Status::Forbidden);
        }
        if !result
            .hostname
            .as_ref()
            .is_some_and(|host| self.hostnames.contains(host))
        {
            return Err(Status::Forbidden);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Header;
    use rocket::local::asynchronous::Client;
    use rocket::tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[get("/")]
    fn credentials(value: ProbeCredentials<'_>) -> &'static str {
        if value.0.is_some() { "key" } else { "captcha" }
    }

    #[rocket::async_test]
    async fn bearer_header_is_strict_and_optional() {
        let client = Client::tracked(rocket::build().mount("/", routes![credentials]))
            .await
            .unwrap();
        assert_eq!(
            client
                .get("/")
                .dispatch()
                .await
                .into_string()
                .await
                .as_deref(),
            Some("captcha")
        );
        for header in ["Bearer key", "bearer key"] {
            assert_eq!(
                client
                    .get("/")
                    .header(Header::new("Authorization", header))
                    .dispatch()
                    .await
                    .into_string()
                    .await
                    .as_deref(),
                Some("key")
            );
        }
        for header in ["Basic key", "Bearer", "Bearer ", "Bearer a b"] {
            assert_eq!(
                client
                    .get("/")
                    .header(Header::new("Authorization", header))
                    .dispatch()
                    .await
                    .status(),
                Status::Unauthorized
            );
        }
        assert_eq!(
            client
                .get("/")
                .header(Header::new("Authorization", "Bearer first"))
                .header(Header::new("Authorization", "Bearer second"))
                .dispatch()
                .await
                .status(),
            Status::Unauthorized
        );
    }

    fn auth(endpoint: String) -> ProbeAuth {
        ProbeAuth {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap(),
            secret: Some("test-secret".into()),
            hostnames: HashSet::from(["example.com".into()]),
            endpoint,
        }
    }

    #[rocket::async_test]
    async fn rejects_missing_tokens_and_configuration_without_network() {
        let mut auth = auth("http://127.0.0.1:1".into());
        for token in [None, Some(""), Some("x".repeat(2049).as_str())] {
            assert_eq!(auth.verify_turnstile(token).await, Err(Status::Forbidden));
        }
        auth.secret = None;
        assert_eq!(
            auth.verify_turnstile(Some("token")).await,
            Err(Status::ServiceUnavailable)
        );
        auth.secret = Some("test-secret".into());
        auth.hostnames.clear();
        assert_eq!(
            auth.verify_turnstile(Some("token")).await,
            Err(Status::ServiceUnavailable)
        );
    }

    #[rocket::async_test]
    async fn siteverify_checks_success_action_hostname_and_upstream_failures() {
        for (status, body, expected) in [
            (
                200,
                r#"{"success":true,"action":"probe","hostname":"example.com"}"#,
                Ok(()),
            ),
            (
                200,
                r#"{"success":false,"error-codes":["timeout-or-duplicate"]}"#,
                Err(Status::Forbidden),
            ),
            (
                200,
                r#"{"success":true,"action":"login","hostname":"example.com"}"#,
                Err(Status::Forbidden),
            ),
            (
                200,
                r#"{"success":true,"action":"probe","hostname":"localhost"}"#,
                Err(Status::Forbidden),
            ),
            (200, r#"{"success":true}"#, Err(Status::Forbidden)),
            (
                200,
                r#"{"success":false,"error-codes":["invalid-input-response"]}"#,
                Err(Status::Forbidden),
            ),
            (
                200,
                r#"{"success":false,"error-codes":["invalid-input-secret"]}"#,
                Err(Status::ServiceUnavailable),
            ),
            (
                200,
                r#"{"success":false,"error-codes":["missing-input-secret"]}"#,
                Err(Status::ServiceUnavailable),
            ),
            (
                200,
                r#"{"success":false,"error-codes":["internal-error"]}"#,
                Err(Status::ServiceUnavailable),
            ),
            (
                200,
                r#"{"success":false,"error-codes":["unrecognized-code"]}"#,
                Err(Status::Forbidden),
            ),
            (200, "invalid json", Err(Status::ServiceUnavailable)),
            (500, "{}", Err(Status::ServiceUnavailable)),
        ] {
            let listener = rocket::tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .unwrap();
            let endpoint = format!("http://{}/siteverify", listener.local_addr().unwrap());
            let server = rocket::tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                    if String::from_utf8_lossy(&request)
                        .contains("secret=test-secret&response=test-token")
                    {
                        break;
                    }
                }
                assert!(request.starts_with(b"POST /siteverify HTTP/1.1"));
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            });
            assert_eq!(
                auth(endpoint).verify_turnstile(Some("test-token")).await,
                expected
            );
            server.await.unwrap();
        }
        assert_eq!(
            auth("http://127.0.0.1:1".into())
                .verify_turnstile(Some("test-token"))
                .await,
            Err(Status::ServiceUnavailable)
        );
    }

    #[rocket::async_test]
    #[ignore = "requires disposable PostgreSQL in PROBE_AUTH_TEST_DATABASE_URL"]
    async fn postgres_keys_can_be_used_expired_and_revoked() {
        let url = std::env::var("PROBE_AUTH_TEST_DATABASE_URL").unwrap();
        let admin = PgPool::connect(&url).await.unwrap();
        let schema = format!("probe_auth_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let schema_for_pool = schema.clone();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .after_connect(move |conn, _| {
                let sql = format!("SET search_path TO {schema_for_pool}");
                Box::pin(async move {
                    sqlx::query(&sql).execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261105000000_probe_api_keys.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        // Exercise the same hashing expression documented for key issuance.
        sqlx::query("INSERT INTO probe_api_keys(name, key_hash) VALUES ('test', encode(sha256(convert_to($1, 'UTF8')), 'hex'))")
            .bind("test-api-key").execute(&pool).await.unwrap();
        let mut auth = auth("http://127.0.0.1:1".into());
        auth.secret = None;
        for days in [None, Some(90)] {
            let issued = crate::admin::issue_probe_api_key(
                crate::admin::Admin,
                rocket::serde::json::Json(crate::admin::ProbeApiKeyInput {
                    name: " monitoring ".into(),
                    expires_in_days: days,
                }),
                rocket::State::from(&pool),
            )
            .await
            .unwrap()
            .0
            .into_inner();
            assert_eq!(issued.name, "monitoring");
            assert_eq!(issued.expires_at.is_some(), days.is_some());
            assert!(issued.token.starts_with("probe_"));
            assert_eq!(
                auth.authorize(&pool, ProbeCredentials(Some(&issued.token)), None)
                    .await,
                Ok(Some(issued.id))
            );
            let stored: String =
                sqlx::query_scalar("SELECT key_hash FROM probe_api_keys WHERE id=$1")
                    .bind(issued.id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                stored,
                format!("{:x}", Sha256::digest(issued.token.as_bytes()))
            );
            let listed =
                crate::admin::list_probe_api_keys(crate::admin::Admin, rocket::State::from(&pool))
                    .await
                    .unwrap()
                    .0
                    .into_inner();
            assert!(
                listed
                    .iter()
                    .any(|row| row.id == issued.id && row.revoked_at.is_none())
            );
            let serialized = rocket::serde::json::serde_json::to_value(&listed).unwrap();
            assert!(
                serialized
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row.get("token").is_none() && row.get("key_hash").is_none())
            );
            let revoked = crate::admin::revoke_probe_api_key(
                crate::admin::Admin,
                &issued.id.to_string(),
                rocket::State::from(&pool),
            )
            .await
            .unwrap()
            .0
            .into_inner();
            assert!(revoked.revoked_at.is_some());
            assert_eq!(
                auth.authorize(&pool, ProbeCredentials(Some(&issued.token)), None)
                    .await,
                Err(Status::Unauthorized)
            );
            let repeated = crate::admin::revoke_probe_api_key(
                crate::admin::Admin,
                &issued.id.to_string(),
                rocket::State::from(&pool),
            )
            .await
            .unwrap()
            .0
            .into_inner();
            assert_eq!(repeated.revoked_at, revoked.revoked_at);
            let listed =
                crate::admin::list_probe_api_keys(crate::admin::Admin, rocket::State::from(&pool))
                    .await
                    .unwrap()
                    .0
                    .into_inner();
            assert!(
                listed
                    .iter()
                    .any(|row| row.id == issued.id && row.revoked_at == revoked.revoked_at)
            );
        }
        assert_eq!(
            crate::admin::revoke_probe_api_key(
                crate::admin::Admin,
                &uuid::Uuid::new_v4().to_string(),
                rocket::State::from(&pool)
            )
            .await
            .err(),
            Some(Status::NotFound)
        );
        let key_id: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM probe_api_keys WHERE name = 'test'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            auth.authorize(&pool, ProbeCredentials(Some("test-api-key")), None)
                .await,
            Ok(Some(key_id))
        );
        assert_eq!(
            auth.authorize(&pool, ProbeCredentials(Some("wrong")), Some("token"))
                .await,
            Err(Status::Unauthorized)
        );
        sqlx::query("UPDATE probe_api_keys SET expires_at = now() - interval '1 second'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            auth.authorize(&pool, ProbeCredentials(Some("test-api-key")), None)
                .await,
            Err(Status::Unauthorized)
        );
        sqlx::query("UPDATE probe_api_keys SET expires_at = NULL, revoked_at = now()")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            auth.authorize(&pool, ProbeCredentials(Some("test-api-key")), None)
                .await,
            Err(Status::Unauthorized)
        );
        pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&admin)
            .await
            .unwrap();
        assert_eq!(
            auth.authorize(&pool, ProbeCredentials(Some("test-api-key")), None)
                .await,
            Err(Status::ServiceUnavailable)
        );
    }
}
