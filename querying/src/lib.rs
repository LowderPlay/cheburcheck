use crate::geoip::{GeoIp, IpInfo};
use crate::lists::{CdnList, NetworkRecord, RuBlacklist};
use crate::resolver::{ResolveError, Resolver};
use crate::target::Target;
use crate::updater::Updatable;
use chrono::{DateTime, Utc};
use ipnet::IpNet;
use log::error;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use maxminddb::MaxMindDbError;
use thiserror::Error;
use tokio::sync::{watch, RwLock};

pub mod geoip;
pub mod lists;
pub mod resolver;
pub mod updater;
pub mod target;

pub struct Checker {
    rx: watch::Receiver<Option<DateTime<Utc>>>,
    tx: watch::Sender<Option<DateTime<Utc>>>,
    cdn_list: Arc<RwLock<CdnList>>,
    ru_blacklist: Arc<RwLock<RuBlacklist>>,
    geo_ip: Arc<RwLock<GeoIp>>,
    resolver: Resolver,
}

pub struct Check {
    pub verdict: CheckVerdict,
    pub geo: IpInfo,
    pub ips: Vec<IpAddr>,
}

pub enum CheckVerdict {
    Clear,
    Blocked {
        rkn_domain: Option<String>,
        rkn_subnets: HashSet<IpNet>,
        cdn_provider_subnets: HashMap<String, HashSet<NetworkRecord>>,
    },
}

#[derive(Debug, Error)]
pub enum CheckError {
    #[error("resolve error")]
    ResolveError(#[from] ResolveError),
    #[error("geoip error")]
    GeoIpError,
    #[error("domain not found")]
    NotFound,
}

impl Checker {
    pub async fn new() -> Checker {
        let (tx, rx) = watch::channel(None);

        Checker {
            rx,
            tx,
            cdn_list: Arc::new(RwLock::new(CdnList::new())),
            ru_blacklist: Arc::new(RwLock::new(RuBlacklist::new())),
            geo_ip: Arc::new(RwLock::new(GeoIp::new())),
            resolver: Resolver::new().await,
        }
    }

    pub async fn geo_ip(&self, ip: IpAddr) -> Result<IpInfo, MaxMindDbError> {
        self.geo_ip.read().await.lookup(ip)
    }

    pub async fn check(&self, target: Target) -> Result<Check, CheckError> {
        let ips = match target.resolve(&self.resolver).await {
            Ok(ips) => ips,
            Err(ResolveError::NxDomain) => {
                return Err(CheckError::NotFound);
            }
            Err(e) => {
                error!("{}", e);
                return Err(CheckError::ResolveError(e));
            },
        };
        let geo_ip = self.geo_ip.read().await;
        let geo = match ips.get(0).map(|ip| geo_ip.lookup(ip.clone())) {
            None => IpInfo::default(),
            Some(Ok(ip)) => ip,
            Some(Err(e)) => {
                error!("{}", e);
                return Err(CheckError::GeoIpError);
            },
        };
        let mut cdn_provider_subnets: HashMap<String, HashSet<NetworkRecord>> = HashMap::new();

        let cdn_list = self.cdn_list.read().await;
        ips.iter()
            .filter_map(|ip| cdn_list.contains(ip))
            .map(|ip| (match &ip.region {
                None => ip.provider.clone(),
                Some(region) => format!("{} ({})", ip.provider, region),
            }, ip.clone()))
            .for_each(|(k, v)| {
                cdn_provider_subnets.entry(k).or_default().insert(v);
            });

        let ru_blacklist = self.ru_blacklist.read().await;
        let domain = match &target {
            Target::Domain(domain) => ru_blacklist.contains_domain(domain),
            _ => None
        };

        let rkn_subnets: HashSet<IpNet> = ips.iter()
            .filter_map(|ip| ru_blacklist.contains_ip(ip))
            .collect();

        Ok(Check {
            verdict: match (domain, cdn_provider_subnets.is_empty(), rkn_subnets.is_empty()) {
                (None, true, true) => CheckVerdict::Clear,
                (domain, _, _) => CheckVerdict::Blocked {
                    rkn_domain: domain,
                    rkn_subnets,
                    cdn_provider_subnets,
                }
            },
            geo,
            ips
        })
    }

    pub fn last_update(&self) -> Option<DateTime<Utc>> {
        self.rx.borrow().clone()
    }

    pub async fn update_all(&self) {
        match GeoIp::download().await {
            Ok(base) => {
                if let Err(e) = self.geo_ip.write().await.install(base).await {
                    error!("Failed to update GeoIP: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to download GeoIP: {}", e);
            }
        }
        match RuBlacklist::download().await {
            Ok(base) => {
                if let Err(e) = self.ru_blacklist.write().await.install(base).await {
                    error!("Failed to update RKN: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to download RKN: {}", e);
            }
        }

        match CdnList::download().await {
            Ok(base) => {
                if let Err(e) = self.cdn_list.write().await.install(base).await {
                    error!("Failed to update CDN: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to download CDN: {}", e);
            }
        }
        self.tx.send(Some(Utc::now())).unwrap();
    }

    pub async fn total_domains(&self) -> usize {
        self.ru_blacklist.read().await.domain_count
    }

    pub async fn total_v4s(&self) -> usize {
        (self.cdn_list.read().await.v4_count() + self.ru_blacklist.read().await.v4_count()) as usize
    }

}
