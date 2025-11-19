use std::io;
use std::io::Error;
use std::sync::Arc;
use reqwest::IntoUrl;
use rocket::tokio::sync::RwLock;
use crate::geoip::GeoIp;
use crate::lists::{CdnList, RuBlacklist};

pub async fn fetch_db<T: IntoUrl>(url: T) -> Result<Vec<u8>, Error> {
    let response = reqwest::get(url).await
        .map_err(|e| Error::new(io::ErrorKind::Other, e))?
        .error_for_status()
        .map_err(|e| Error::new(io::ErrorKind::Other, e))?;
    let bytes = response.bytes().await
        .map_err(|e| Error::new(io::ErrorKind::Other, e))?;
    Ok(bytes.to_vec())
}

#[async_trait]
pub trait Updatable {
    type Base;
    async fn download() -> Result<Self::Base, Error>;
    async fn install(&mut self, base: Self::Base) -> Result<(), Error>;
    fn get_url(key: &'static str, default: &'static str) -> String {
        std::env::var(key).ok().unwrap_or(default.to_string())
    }
}

pub async fn update_all(geo_ip: Arc<RwLock<GeoIp>>, rkn: Arc<RwLock<RuBlacklist>>, cdn: Arc<RwLock<CdnList>>) {
    info!("Updating all DBs");
    match GeoIp::download().await {
        Ok(base) => {
            if let Err(e) = geo_ip.write().await.install(base).await {
                error!("Failed to update GeoIP: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to download GeoIP: {}", e);
        }
    }
    match RuBlacklist::download().await {
        Ok(base) => {
            if let Err(e) = rkn.write().await.install(base).await {
                error!("Failed to update RKN: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to download RKN: {}", e);
        }
    }

    match CdnList::download().await {
        Ok(base) => {
            if let Err(e) = cdn.write().await.install(base).await {
                error!("Failed to update CDN: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to download CDN: {}", e);
        }
    }
    info!("Updated databases");
}
