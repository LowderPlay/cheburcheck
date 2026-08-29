use crate::Bases;
use crate::geoip::GeoIp;
use crate::lists::{CdnList, RuBlacklist};
use crate::updater::Updatable;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

const METADATA_FILE: &str = "metadata.json";
const GEO_ASN_FILE: &str = "geo-asn.mmdb";
const GEO_COUNTRY_FILE: &str = "geo-country.mmdb";
const GEO_CITY_FILE: &str = "geo-city.mmdb";
const RKN_NETS_FILE: &str = "rkn-nets.lst";
const RKN_DOMAINS_FILE: &str = "rkn-domains.lst";
const CDN_FILE: &str = "cdn.csv";

#[derive(Debug, Clone)]
pub struct DatabaseCache {
    path: PathBuf,
}

#[derive(Deserialize, Serialize)]
struct CacheMetadata {
    updated_at: DateTime<Utc>,
    sources: Vec<CacheSourceMetadata>,
}

#[derive(Deserialize, Serialize)]
struct CacheSourceMetadata {
    name: String,
    url: String,
    file: String,
    bytes: usize,
}

impl DatabaseCache {
    pub fn from_env() -> Self {
        Self {
            path: std::env::var("DATABASE_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("database-cache")),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> io::Result<Bases> {
        Ok((
            (
                fs::read(self.path.join(GEO_ASN_FILE))?,
                fs::read(self.path.join(GEO_COUNTRY_FILE))?,
                fs::read(self.path.join(GEO_CITY_FILE))?,
            ),
            (
                VecDeque::from(fs::read(self.path.join(RKN_NETS_FILE))?),
                VecDeque::from(fs::read(self.path.join(RKN_DOMAINS_FILE))?),
                VecDeque::from(include_bytes!("../dist-domains.txt").to_vec()),
            ),
            VecDeque::from(fs::read(self.path.join(CDN_FILE))?),
        ))
    }

    pub fn updated_at(&self) -> io::Result<DateTime<Utc>> {
        let metadata = fs::read(self.path.join(METADATA_FILE))?;
        let metadata: CacheMetadata = serde_json::from_slice(&metadata)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(metadata.updated_at)
    }

    pub fn refresh_delay(&self, interval: Duration) -> io::Result<Duration> {
        let elapsed = Utc::now()
            .signed_duration_since(self.updated_at()?)
            .to_std()
            .unwrap_or(Duration::ZERO);
        Ok(interval.saturating_sub(elapsed))
    }

    pub fn store(&self, bases: &Bases) -> io::Result<()> {
        fs::create_dir_all(&self.path)?;

        let (geo_ip, ru_blacklist, cdn_list) = bases;
        self.write_file(GEO_ASN_FILE, &geo_ip.0)?;
        self.write_file(GEO_COUNTRY_FILE, &geo_ip.1)?;
        self.write_file(GEO_CITY_FILE, &geo_ip.2)?;
        self.write_vec_deque_file(RKN_NETS_FILE, &ru_blacklist.0)?;
        self.write_vec_deque_file(RKN_DOMAINS_FILE, &ru_blacklist.1)?;
        self.write_vec_deque_file(CDN_FILE, cdn_list)?;

        let metadata = CacheMetadata {
            updated_at: Utc::now(),
            sources: vec![
                CacheSourceMetadata {
                    name: "geo_asn".to_string(),
                    url: GeoIp::get_url("GEO_ASN", "https://git.io/GeoLite2-ASN.mmdb"),
                    file: GEO_ASN_FILE.to_string(),
                    bytes: geo_ip.0.len(),
                },
                CacheSourceMetadata {
                    name: "geo_country".to_string(),
                    url: GeoIp::get_url("GEO_COUNTRY", "https://git.io/GeoLite2-Country.mmdb"),
                    file: GEO_COUNTRY_FILE.to_string(),
                    bytes: geo_ip.1.len(),
                },
                CacheSourceMetadata {
                    name: "geo_city".to_string(),
                    url: GeoIp::get_url("GEO_CITY", "https://git.io/GeoLite2-City.mmdb"),
                    file: GEO_CITY_FILE.to_string(),
                    bytes: geo_ip.2.len(),
                },
                CacheSourceMetadata {
                    name: "rkn_nets".to_string(),
                    url: RuBlacklist::get_url(
                        "RKN_NETS",
                        "https://antifilter.network/download/ipsum.lst",
                    ),
                    file: RKN_NETS_FILE.to_string(),
                    bytes: ru_blacklist.0.len(),
                },
                CacheSourceMetadata {
                    name: "rkn_domains".to_string(),
                    url: RuBlacklist::get_url(
                        "RKN_DOMAINS",
                        "https://antifilter.download/list/domains.lst",
                    ),
                    file: RKN_DOMAINS_FILE.to_string(),
                    bytes: ru_blacklist.1.len(),
                },
                CacheSourceMetadata {
                    name: "cdn".to_string(),
                    url: CdnList::get_url(
                        "CDN_SOURCE",
                        "https://raw.githubusercontent.com/123jjck/cdn-ip-ranges/refs/heads/main/all/all.csv",
                    ),
                    file: CDN_FILE.to_string(),
                    bytes: cdn_list.len(),
                },
            ],
        };
        let metadata = serde_json::to_vec_pretty(&metadata).map_err(io::Error::other)?;
        self.write_file(METADATA_FILE, &metadata)
    }

    fn write_file(&self, file_name: &str, contents: &[u8]) -> io::Result<()> {
        let path = self.path.join(file_name);
        let tmp_path = self.path.join(format!("{file_name}.tmp"));
        fs::write(&tmp_path, contents)?;
        fs::rename(tmp_path, path)
    }

    fn write_vec_deque_file(&self, file_name: &str, contents: &VecDeque<u8>) -> io::Result<()> {
        let bytes: Vec<u8> = contents.iter().copied().collect();
        self.write_file(file_name, &bytes)
    }
}
