use hickory_resolver::config::{LookupIpStrategy, ResolverConfig, ResolverOpts};
use hickory_resolver::net::{DnsError, NetError};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::ProtoError;
use std::io::{Error, ErrorKind};
use std::net::IpAddr;
use std::sync::Arc;
use thiserror::Error;

use crate::asn::{AsnCache, AsnError};

pub struct Resolver {
    resolver: hickory_resolver::Resolver<TokioRuntimeProvider>,
    pub asn_cache: Arc<AsnCache>,
}

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("domain not found")]
    NxDomain,
    #[error("resolver error")]
    Other(#[from] Error),
    #[error("not implemented")]
    NotImplemented,
    #[error("asn not found")]
    AsnNotFound,
    #[error("asn network error: {0}")]
    AsnNetworkError(String),
    #[error("asn parse error: {0}")]
    AsnParseError(String),
}

impl From<AsnError> for ResolveError {
    fn from(err: AsnError) -> Self {
        match err {
            AsnError::NotFound => ResolveError::AsnNotFound,
            AsnError::NetworkError(msg) => ResolveError::AsnNetworkError(msg),
            AsnError::ParseError(msg) => ResolveError::AsnParseError(msg),
        }
    }
}

impl Resolver {
    pub async fn new() -> Resolver {
        let config = ResolverConfig::https(&hickory_resolver::config::QUAD9);
        let mut opts = ResolverOpts::default();
        opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
        let resolver = hickory_resolver::Resolver::builder_with_config(config, TokioRuntimeProvider::default())
            .with_options(opts)
            .build().expect("build resolver");
        Resolver { 
            resolver,
            asn_cache: Arc::new(AsnCache::new()),
        }
    }

    pub async fn lookup_ips(&self, domain: &str) -> Result<Vec<IpAddr>, ResolveError> {
        Ok(self.resolver.lookup_ip(domain).await
            .map_err(|e| match e {
                NetError::Dns(DnsError::NoRecordsFound(..)) => ResolveError::NxDomain,
                NetError::Proto(ProtoError::Msg(msg)) 
                    if msg.contains("Malformed label") || 
                       msg.contains("invalid characters") => ResolveError::NxDomain,
                _ => ResolveError::Other(Error::new(ErrorKind::Other, e))
            })?.iter().collect())
    }
}
