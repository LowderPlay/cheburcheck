use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

#[derive(Deserialize)]
pub struct RipeStatResponse {
    pub data: RipeStatData,
}

#[derive(Deserialize)]
pub struct RipeStatData {
    pub prefixes: Vec<Prefix>,
}

#[derive(Deserialize)]
pub struct Prefix {
    pub prefix: String,
}

#[derive(Debug)]
pub enum AsnError {
    NotFound,
    NetworkError(String),
    ParseError(String),
}

impl From<reqwest::Error> for AsnError {
    fn from(err: reqwest::Error) -> Self {
        AsnError::NetworkError(err.to_string())
    }
}

pub struct CachedAsnData {
    pub prefixes: Vec<String>,
    pub cached_at: SystemTime,
}

impl CachedAsnData {
    pub fn new(prefixes: Vec<String>) -> Self {
        Self {
            prefixes,
            cached_at: SystemTime::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map(|d| d.as_secs() > 86400)
            .unwrap_or(true)
    }
}

pub async fn fetch_asn_prefixes(asn: u32) -> Result<Vec<String>, AsnError> {
    let url = format!(
        "https://stat.ripe.net/data/announced-prefixes/data.json?resource=AS{}",
        asn
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client.get(&url).send().await?;

    if response.status() == 404 {
        return Err(AsnError::NotFound);
    }

    let data: RipeStatResponse = response
        .json()
        .await
        .map_err(|e| AsnError::ParseError(e.to_string()))?;

    Ok(data.data.prefixes.into_iter().map(|p| p.prefix).collect())
}

pub async fn fetch_asn_prefixes_cached(
    asn: u32,
    get_cached: impl FnOnce(u32) -> Option<Vec<String>>,
    cache_result: impl FnOnce(u32, Vec<String>),
) -> Result<Vec<String>, AsnError> {
    if let Some(cached_prefixes) = get_cached(asn) {
        return Ok(cached_prefixes);
    }

    let prefixes = fetch_asn_prefixes(asn).await?;

    cache_result(asn, prefixes.clone());

    Ok(prefixes)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AsnInfo {
    pub asn: u32,
    pub prefixes: Vec<String>,
    pub blocked_prefixes: Vec<String>,
}

impl AsnInfo {
    pub fn new(asn: u32, prefixes: Vec<String>, blocked_prefixes: Vec<String>) -> Self {
        Self {
            asn,
            prefixes,
            blocked_prefixes,
        }
    }

    pub fn ipv4_prefixes(&self) -> Vec<&str> {
        self.prefixes
            .iter()
            .filter(|p| !p.contains(':'))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn ipv6_prefixes(&self) -> Vec<&str> {
        self.prefixes
            .iter()
            .filter(|p| p.contains(':'))
            .map(|s| s.as_str())
            .collect()
    }
}

pub struct AsnCache {
    cache: Arc<RwLock<HashMap<u32, CachedAsnData>>>,
}

impl Default for AsnCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AsnCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_cached_asn(&self, asn: u32) -> Option<Vec<String>> {
        let cache = self.cache.read().ok()?;
        let cached_data = cache.get(&asn)?;

        if cached_data.is_expired() {
            None
        } else {
            Some(cached_data.prefixes.clone())
        }
    }

    pub fn cache_asn(&self, asn: u32, prefixes: Vec<String>) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(asn, CachedAsnData::new(prefixes));
        }
    }
}
