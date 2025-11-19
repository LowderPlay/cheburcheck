use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use hickory_resolver::proto::ProtoError;
use url::Url;
use crate::resolver::Resolver;

pub enum Target {
    Domain(String),
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
}

impl From<&str> for Target {
    fn from(input: &str) -> Self {
        if let Ok(ipv4) = input.parse::<Ipv4Addr>() {
            return Target::Ipv4(ipv4);
        }

        if let Ok(ipv6) = input.parse::<Ipv6Addr>() {
            return Target::Ipv6(ipv6);
        }

        if let Ok(url) = input.parse::<Url>() {
            if let Some(host) = url.host_str() {
                return Target::Domain(host.to_string());
            }
        }
        Target::Domain(input.to_string())
    }
}

impl Target {
    pub fn readable_type(&self) -> &'static str {
        match self {
            Target::Domain(_) => "Домен",
            Target::Ipv4(_) => "IPv4-адрес",
            Target::Ipv6(_) => "IPv6-адрес"
        }
    }

    pub async fn resolve(&self, resolver: &Resolver) -> Result<Vec<IpAddr>, ProtoError> {
        Ok(match self {
            Target::Domain(domain) => resolver.lookup_ips(domain).await?,
            Target::Ipv4(ipv4) => vec![IpAddr::V4(ipv4.clone())],
            Target::Ipv6(ipv6) => vec![IpAddr::V6(ipv6.clone())],
        })
    }

    pub fn to_query(&self) -> String {
        match self {
            Target::Domain(domain) => domain.clone(),
            Target::Ipv4(v4) => v4.to_string(),
            Target::Ipv6(v6) => v6.to_string(),
        }
    }
}
