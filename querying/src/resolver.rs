use hickory_resolver::config::{LookupIpStrategy, ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::TokioConnectionProvider;
use std::io::{Error, ErrorKind};
use std::net::IpAddr;
use thiserror::Error;

pub struct Resolver {
    resolver: hickory_resolver::Resolver<TokioConnectionProvider>,
}

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("domain not found")]
    NxDomain,
    #[error("resolver error")]
    Other(#[from] Error),
}

impl Resolver {
    pub async fn new() -> Resolver {
        let config = ResolverConfig::quad9_https();
        let mut opts = ResolverOpts::default();
        opts.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
        let resolver = hickory_resolver::Resolver::builder_with_config(config, TokioConnectionProvider::default())
            .with_options(opts)
            .build();
        Resolver { resolver }
    }

    pub async fn lookup_ips(&self, domain: &str) -> Result<Vec<IpAddr>, ResolveError> {
        Ok(self.resolver.lookup_ip(domain).await
            .map_err(|e| if e.kind.is_no_records_found() {
                ResolveError::NxDomain
            } else {
                ResolveError::Other(Error::new(ErrorKind::Other, e))
            })?
            .into_iter().collect())
    }
}
