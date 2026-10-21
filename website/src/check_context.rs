use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rocket::http::Status;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const AUDIENCE: &str = "check-context";
const LIFETIME_SECONDS: u64 = 24 * 60 * 60;

// Both probe and feedback intentionally accept this check context. It carries no
// reporter credentials or authority to select a privileged probe.
pub struct CheckContext {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub query_id: Uuid,
    pub target: String,
    pub ip: Option<IpAddr>,
    aud: String,
    iat: u64,
    exp: u64,
}

impl CheckContext {
    pub fn from_env() -> Result<Self> {
        let secret = std::env::var("CHECK_CONTEXT_SECRET")
            .context("CHECK_CONTEXT_SECRET must be set to a random secret of at least 32 bytes")?;
        Self::new(secret.as_bytes())
    }

    fn new(secret: &[u8]) -> Result<Self> {
        anyhow::ensure!(
            secret.len() >= 32,
            "CHECK_CONTEXT_SECRET must be at least 32 bytes"
        );
        Ok(Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        })
    }

    pub fn issue(&self, query_id: Uuid, target: String, ip: Option<IpAddr>) -> Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                query_id,
                target,
                ip,
                aud: AUDIENCE.into(),
                iat: now,
                exp: now + LIFETIME_SECONDS,
            },
            &self.encoding,
        )?)
    }

    pub fn verify(&self, token: &str) -> Result<Claims, Status> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&[AUDIENCE]);
        validation.set_required_spec_claims(&["exp", "aud"]);
        validation.leeway = 0;
        let claims = decode::<Claims>(token, &self.decoding, &validation)
            .map_err(|_| Status::Unauthorized)?
            .claims;
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> CheckContext {
        CheckContext::new(b"test-only-secret-with-at-least-32-bytes").unwrap()
    }

    #[test]
    fn signed_metadata_round_trips() {
        let signer = signer();
        let id = Uuid::new_v4();
        let ip = "1.2.3.4".parse().unwrap();
        let token = signer.issue(id, "example.org".into(), Some(ip)).unwrap();
        let claims = signer.verify(&token).unwrap();
        assert_eq!(claims.target, "example.org");
        assert_eq!(claims.ip, Some(ip));
        assert_eq!(claims.query_id, id);
        assert!(signer.verify(&id.to_string()).is_err());
        let other = CheckContext::new(b"another-test-only-secret-with-32-bytes").unwrap();
        assert!(other.verify(&token).is_err());
    }

    #[test]
    fn rejects_tampering_expiry_wrong_audience_and_algorithm() {
        let signer = signer();
        let id = Uuid::new_v4();
        let token = signer.issue(id, "example.org".into(), None).unwrap();
        let mut claims = signer.verify(&token).unwrap();
        claims.target = "attacker.example".into();
        let modified = encode(&Header::new(Algorithm::HS256), &claims, &signer.encoding).unwrap();
        // Replace only the payload, retaining the original signature.
        let parts: Vec<_> = token.split('.').collect();
        let payload = modified.split('.').nth(1).unwrap();
        assert!(
            signer
                .verify(&format!("{}.{payload}.{}", parts[0], parts[2]))
                .is_err()
        );
        let wrong_algorithm =
            encode(&Header::new(Algorithm::HS384), &claims, &signer.encoding).unwrap();
        assert!(signer.verify(&wrong_algorithm).is_err());
        claims.aud = "reporter".into();
        let wrong_audience =
            encode(&Header::new(Algorithm::HS256), &claims, &signer.encoding).unwrap();
        assert!(signer.verify(&wrong_audience).is_err());
        claims.aud = AUDIENCE.into();
        claims.exp = 1;
        let expired = encode(&Header::new(Algorithm::HS256), &claims, &signer.encoding).unwrap();
        assert!(signer.verify(&expired).is_err());
    }

    #[test]
    fn rejects_short_signing_secrets() {
        assert!(CheckContext::new(b"short").is_err());
    }
}
