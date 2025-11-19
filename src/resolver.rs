use std::net::IpAddr;
use hickory_resolver::config::{LookupIpStrategy, ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::{TokioConnectionProvider};
use hickory_resolver::proto::ProtoError;

pub struct Resolver {
    resolver: hickory_resolver::Resolver<TokioConnectionProvider>,
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

    pub async fn lookup_ips(&self, domain: &str) -> Result<Vec<IpAddr>, ProtoError> {
        Ok(self.resolver.lookup_ip(domain).await?
            .into_iter().collect())
    }
}
