use crate::asn::fetch_asn_prefixes_cached;
use crate::resolver::{ResolveError, Resolver};
use crate::{sample_ipv4_subnet, sample_ipv6_subnet};
use ipnet::{Ipv4Net, Ipv6Net};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

#[derive(Debug, Clone)]
pub enum Target {
    Domain(String),
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    Ipv4Subnet(Ipv4Net),
    Ipv6Subnet(Ipv6Net),
    Asn(u32),
}

impl From<&str> for Target {
    fn from(input: &str) -> Self {
        if input.to_lowercase().starts_with("as") {
            if let Ok(asn) = input[2..].parse::<u32>() {
                if asn >= 1 {
                    return Target::Asn(asn);
                }
            }
        }

        if input.contains('/') {
            if let Ok(ipv4_net) = input.parse::<Ipv4Net>() {
                if ipv4_net.prefix_len() >= 8 {
                    return Target::Ipv4Subnet(ipv4_net);
                }
            }

            if let Ok(ipv6_net) = input.parse::<Ipv6Net>() {
                if ipv6_net.prefix_len() >= 32 {
                    return Target::Ipv6Subnet(ipv6_net);
                }
            }
        }

        if let Ok(ipv4) = input.parse::<Ipv4Addr>() {
            return Target::Ipv4(ipv4);
        }

        if let Ok(ipv6) = input.parse::<Ipv6Addr>() {
            return Target::Ipv6(ipv6);
        }

        if let Ok(url) = input.parse::<Url>() {
            if let Some(host) = url.host_str() {
                return Target::Domain(host.trim_end_matches('.').to_string());
            }
        }
        Target::Domain(input.trim_end_matches('.').to_string())
    }
}

impl Target {
    pub fn readable_type(&self) -> &'static str {
        match self {
            Target::Domain(_) => "Домен",
            Target::Ipv4(_) => "IPv4-адрес",
            Target::Ipv6(_) => "IPv6-адрес",
            Target::Ipv4Subnet(_) => "IPv4-подсеть",
            Target::Ipv6Subnet(_) => "IPv6-подсеть",
            Target::Asn(_) => "Автономная система",
        }
    }

    pub async fn resolve(&self, resolver: &Resolver) -> Result<Vec<IpAddr>, ResolveError> {
        Ok(match self {
            Target::Domain(domain) => resolver.lookup_ips(domain).await?,
            Target::Ipv4(ipv4) => vec![IpAddr::V4(*ipv4)],
            Target::Ipv6(ipv6) => vec![IpAddr::V6(*ipv6)],
            Target::Ipv4Subnet(net) => sample_ipv4_subnet(*net),
            Target::Ipv6Subnet(net) => sample_ipv6_subnet(*net),
            Target::Asn(asn) => {
                let mut prefixes = fetch_asn_prefixes_cached(
                    *asn,
                    |asn| resolver.asn_cache.get_cached_asn(asn),
                    |asn, prefixes| resolver.asn_cache.cache_asn(asn, prefixes),
                )
                .await?;

                if prefixes.is_empty() {
                    return Ok(vec![]);
                }

                if prefixes.len() > 100 {
                    prefixes.truncate(100);
                }

                let mut all_ips = Vec::new();

                for prefix in &prefixes {
                    if let Ok(ipv4_net) = prefix.parse::<Ipv4Net>() {
                        all_ips.extend(sample_ipv4_subnet(ipv4_net));
                    } else if let Ok(ipv6_net) = prefix.parse::<Ipv6Net>() {
                        all_ips.extend(sample_ipv6_subnet(ipv6_net));
                    }
                }

                all_ips
            }
        })
    }

    pub fn to_query(&self) -> String {
        match self {
            Target::Domain(domain) => domain.clone(),
            Target::Ipv4(v4) => v4.to_string(),
            Target::Ipv6(v6) => v6.to_string(),
            Target::Ipv4Subnet(net) => net.to_string(),
            Target::Ipv6Subnet(net) => net.to_string(),
            Target::Asn(asn) => format!("AS{}", asn),
        }
    }

    pub fn subnet_size(&self) -> Option<String> {
        match self {
            Target::Ipv4Subnet(net) => {
                let size = 2u64.pow(32 - net.prefix_len() as u32);
                Some(format_large_number(size as u128))
            }
            Target::Ipv6Subnet(net) => {
                if net.prefix_len() == 128 {
                    Some("1".to_string())
                } else {
                    let size = 2u128.pow(128 - net.prefix_len() as u32);
                    Some(format_large_number(size))
                }
            }
            _ => None,
        }
    }
}

fn format_large_number(n: u128) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else if n < 1_000_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n < 1_000_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n < 1_000_000_000_000_000 {
        format!("{:.1}T", n as f64 / 1_000_000_000_000.0)
    } else if n < 1_000_000_000_000_000_000 {
        format!("{:.1}P", n as f64 / 1_000_000_000_000_000.0)
    } else if n < 1_000_000_000_000_000_000_000 {
        format!("{:.1}E", n as f64 / 1_000_000_000_000_000_000.0)
    } else {
        format!("{:.1}Z", n as f64 / 1_000_000_000_000_000_000_000.0)
    }
}
