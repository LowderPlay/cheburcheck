use crate::api::ProbeUpdateDownloadRateLimiter;
use log::error;
use reqwest::Client;
use rocket::State;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use rocket::tokio::sync::Mutex;
use rocket_client_addr::ClientRealAddr;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_REPOSITORY: &str = "LowderPlay/cheburcheck";
const DEFAULT_PUBLIC_BASE_URL: &str = "https://cheburcheck.ru/api/v1/probe-updates";
const DEFAULT_ASSET_CACHE_DIR: &str = "/var/cache/cheburcheck/probe-updates";
const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const MAX_ASSET_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct ProbeUpdateProxy {
    client: Client,
    github_token: Option<String>,
    public_base_url: String,
    cache_ttl: Duration,
    cache: Arc<Mutex<Option<CachedRelease>>>,
    asset_cache_dir: PathBuf,
    asset_cache_ttl: Duration,
    asset_cache_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct CachedRelease {
    fetched_at: Instant,
    release: Release,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GithubAsset {
    id: u64,
    name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Release {
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Serialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(skip)]
    github_url: String,
}

impl ProbeUpdateProxy {
    pub fn from_env() -> Result<Self, reqwest::Error> {
        let public_base_url = std::env::var("PROBE_UPDATE_PUBLIC_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_PUBLIC_BASE_URL.to_owned())
            .trim_end_matches('/')
            .to_owned();
        let cache_seconds = std::env::var("PROBE_UPDATE_CACHE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(300);
        let asset_cache_dir = std::env::var_os("PROBE_UPDATE_ASSET_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_ASSET_CACHE_DIR));
        let asset_cache_seconds = std::env::var("PROBE_UPDATE_ASSET_CACHE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3600);
        let github_token = std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|value| !value.is_empty());
        let client = Client::builder()
            .user_agent(concat!(
                "cheburcheck-update-proxy/",
                env!("CARGO_PKG_VERSION")
            ))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self {
            client,
            github_token,
            public_base_url,
            cache_ttl: Duration::from_secs(cache_seconds),
            cache: Arc::new(Mutex::new(None)),
            asset_cache_dir,
            asset_cache_ttl: Duration::from_secs(asset_cache_seconds),
            asset_cache_lock: Arc::new(Mutex::new(())),
        })
    }

    fn github_request(&self, url: &str, accept: &'static str) -> reqwest::RequestBuilder {
        let request = self.client.get(url).header(reqwest::header::ACCEPT, accept);
        match &self.github_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn release(&self) -> Result<Release, reqwest::Error> {
        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.as_ref()
            && cached.fetched_at.elapsed() < self.cache_ttl
        {
            return Ok(cached.release.clone());
        }

        let url = format!("{GITHUB_API_BASE_URL}/repos/{DEFAULT_REPOSITORY}/releases/latest");
        let github_release = self
            .github_request(&url, "application/vnd.github+json")
            .send()
            .await?
            .error_for_status()?
            .json::<GithubRelease>()
            .await?;
        let release = Release {
            assets: github_release
                .assets
                .into_iter()
                .filter(|asset| valid_asset_name(&asset.name))
                .map(|asset| Asset {
                    browser_download_url: format!("{}/assets/{}", self.public_base_url, asset.name),
                    name: asset.name,
                    github_url: format!(
                        "{GITHUB_API_BASE_URL}/repos/{}/releases/assets/{}",
                        DEFAULT_REPOSITORY, asset.id
                    ),
                })
                .collect(),
        };
        *cache = Some(CachedRelease {
            fetched_at: Instant::now(),
            release: release.clone(),
        });
        Ok(release)
    }

    async fn cached_asset(&self, path: &Path) -> Option<Vec<u8>> {
        let metadata = rocket::tokio::fs::metadata(path).await.ok()?;
        if metadata.len() > MAX_ASSET_BYTES
            || metadata
                .modified()
                .ok()?
                .elapsed()
                .unwrap_or(Duration::ZERO)
                >= self.asset_cache_ttl
        {
            return None;
        }
        rocket::tokio::fs::read(path).await.ok()
    }

    async fn store_asset(&self, path: &Path, bytes: &[u8]) {
        if let Err(error) = rocket::tokio::fs::create_dir_all(&self.asset_cache_dir).await {
            error!("failed to create probe asset cache directory: {error}");
            return;
        }
        let temporary = path.with_extension(format!(
            "{}.tmp",
            path.extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("asset")
        ));
        if let Err(error) = rocket::tokio::fs::write(&temporary, bytes).await {
            error!("failed to write probe asset cache file: {error}");
            return;
        }
        if let Err(error) = rocket::tokio::fs::rename(&temporary, path).await {
            error!("failed to commit probe asset cache file: {error}");
            let _ = rocket::tokio::fs::remove_file(temporary).await;
        }
    }
}

#[get("/releases/latest")]
pub async fn latest_release(proxy: &State<ProbeUpdateProxy>) -> Result<Json<Release>, Status> {
    proxy.release().await.map(Json).map_err(|error| {
        error!("failed to fetch probe release from GitHub: {error}");
        Status::BadGateway
    })
}

#[get("/assets/<name>")]
pub async fn download_asset(
    name: &str,
    addr: &ClientRealAddr,
    proxy: &State<ProbeUpdateProxy>,
    limiter: &State<Arc<ProbeUpdateDownloadRateLimiter>>,
) -> Result<(ContentType, Vec<u8>), Status> {
    if !limiter.check(&addr.ip) {
        return Err(Status::TooManyRequests);
    }
    if !valid_asset_name(name) {
        return Err(Status::BadRequest);
    }
    let release = proxy.release().await.map_err(|error| {
        error!("failed to refresh probe release before asset download: {error}");
        Status::BadGateway
    })?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .ok_or(Status::NotFound)?;
    let cache_path = proxy.asset_cache_dir.join(name);
    if let Some(bytes) = proxy.cached_asset(&cache_path).await {
        return Ok((ContentType::Binary, bytes));
    }
    let _cache_guard = proxy.asset_cache_lock.lock().await;
    if let Some(bytes) = proxy.cached_asset(&cache_path).await {
        return Ok((ContentType::Binary, bytes));
    }
    let upstream = proxy
        .github_request(&asset.github_url, "application/octet-stream")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| {
            error!("failed to download probe asset {}: {error}", asset.name);
            Status::BadGateway
        })?;
    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_ASSET_BYTES)
    {
        error!("probe asset {} exceeds the proxy size limit", asset.name);
        return Err(Status::PayloadTooLarge);
    }
    let bytes = upstream.bytes().await.map_err(|error| {
        error!("failed to read probe asset {}: {error}", asset.name);
        Status::BadGateway
    })?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        error!("probe asset {} exceeds the proxy size limit", asset.name);
        return Err(Status::PayloadTooLarge);
    }
    proxy.store_asset(&cache_path, &bytes).await;
    Ok((ContentType::Binary, bytes.to_vec()))
}

fn valid_asset_name(name: &str) -> bool {
    let safe = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !safe {
        return false;
    }

    let package = name.ends_with(".deb") || name.ends_with(".apk") || name.ends_with(".ipk");
    let standalone = name.ends_with(".exe") || name.contains("-linux-");
    ((name.starts_with("cheburprobe-") || name.starts_with("cheburprobe_"))
        && (package || standalone))
        || ((name.starts_with("luci-app-cheburprobe-")
            || name.starts_with("luci-app-cheburprobe_"))
            && (name.ends_with(".apk") || name.ends_with(".ipk")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_probe_release_asset_names() {
        assert!(valid_asset_name("cheburprobe-0.6.0-linux-amd64"));
        assert!(valid_asset_name("cheburprobe_0.6.0-1_arm64.deb"));
        assert!(valid_asset_name("luci-app-cheburprobe-0.6.0-r1.apk"));
        assert!(!valid_asset_name("cheburchecker.exe"));
        assert!(!valid_asset_name("cheburprobe-0.6.0.sha256"));
        assert!(!valid_asset_name("luci-app-cheburprobe-0.6.0.exe"));
        assert!(!valid_asset_name("../cheburprobe.apk"));
    }
}
