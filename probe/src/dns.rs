use futures::future::join_all;
use hickory_resolver::config::{
    CLOUDFLARE, GOOGLE, LookupIpStrategy, QUAD9, ResolverConfig, ResolverOpts, ServerGroup,
};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::net::{DnsError, NetError};
use reports::probe::{
    DnsObservation, DnsOutcome, DnsProbeResult, DnsProtocol, DnsResponseMetadata,
};
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);
const YANDEX: ServerGroup<'static> = ServerGroup {
    ips: &[IpAddr::V4(Ipv4Addr::new(77, 88, 8, 8))],
    server_name: "common.dot.dns.yandex.net",
    path: "/dns-query",
};

pub async fn check_dns(
    domain: Option<&str>,
    remaining: Duration,
    samples_per_protocol: u8,
    spoofing_provider_threshold: u8,
) -> Option<DnsProbeResult> {
    let domain = domain?;
    let timeout = remaining.min(LOOKUP_TIMEOUT);
    if timeout.is_zero() {
        return None;
    }

    let providers = [
        ("google", &GOOGLE),
        ("cloudflare", &CLOUDFLARE),
        ("quad9", &QUAD9),
        ("yandex", &YANDEX),
    ];
    let mut observations = join_all(providers.into_iter().flat_map(|(provider, servers)| {
        [
            (DnsProtocol::Udp, resolver_config(servers, DnsProtocol::Udp)),
            (DnsProtocol::Tcp, resolver_config(servers, DnsProtocol::Tcp)),
            (DnsProtocol::Doh, ResolverConfig::https(servers)),
            (DnsProtocol::Dot, ResolverConfig::tls(servers)),
        ]
        .map(|(protocol, config)| {
            lookup(
                domain,
                provider,
                protocol,
                config,
                timeout,
                samples_per_protocol,
            )
        })
    }))
    .await;

    mark_suspected_spoofing(&mut observations);
    let suspicious_provider_count = suspicious_provider_count(&observations);
    let spoofing_detected = suspicious_provider_count >= spoofing_provider_threshold;

    Some(DnsProbeResult {
        spoofing_detected,
        suspicious_provider_count,
        verdict_threshold: spoofing_provider_threshold,
        samples_per_protocol,
        observations,
    })
}

fn suspicious_provider_count(observations: &[DnsObservation]) -> u8 {
    ["google", "cloudflare", "quad9", "yandex"]
        .into_iter()
        .filter(|provider| {
            observations.iter().any(|observation| {
                observation.provider == *provider && observation.suspected_spoofing
            })
        })
        .count() as u8
}

fn mark_suspected_spoofing(observations: &mut [DnsObservation]) {
    for provider in ["google", "cloudflare", "quad9", "yandex"] {
        let doh = observations
            .iter()
            .find(|item| item.provider == provider && matches!(item.protocol, DnsProtocol::Doh))
            .filter(|item| successful_outcome(item).is_some());
        let dot = observations
            .iter()
            .find(|item| item.provider == provider && matches!(item.protocol, DnsProtocol::Dot))
            .filter(|item| successful_outcome(item).is_some());
        let Some(reference) = doh
            .zip(dot)
            .and_then(|(doh, dot)| (doh.metadata == dot.metadata).then(|| doh.metadata.clone()))
        else {
            continue;
        };

        for observation in observations.iter_mut().filter(|item| {
            item.provider == provider
                && matches!(item.protocol, DnsProtocol::Udp | DnsProtocol::Tcp)
        }) {
            observation.suspected_spoofing =
                successful_outcome(observation).is_some() && observation.metadata != reference;
        }
    }
}

fn successful_outcome(observation: &DnsObservation) -> Option<&DnsOutcome> {
    (!matches!(observation.outcome, DnsOutcome::Error { .. })).then_some(&observation.outcome)
}

fn resolver_config(servers: &ServerGroup<'_>, protocol: DnsProtocol) -> ResolverConfig {
    let name_servers = match protocol {
        DnsProtocol::Udp => servers.udp().collect(),
        DnsProtocol::Tcp => servers.tcp().collect(),
        DnsProtocol::Doh | DnsProtocol::Dot => unreachable!(),
    };
    ResolverConfig::from_parts(None, Vec::new(), name_servers)
}

async fn lookup(
    domain: &str,
    provider: &str,
    protocol: DnsProtocol,
    config: ResolverConfig,
    timeout: Duration,
    samples_per_protocol: u8,
) -> DnsObservation {
    let mut options = ResolverOpts::default();
    options.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    options.attempts = 1;
    options.timeout = timeout;
    options.cache_size = 0;
    let (outcome, metadata) = match hickory_resolver::Resolver::builder_with_config(
        config,
        TokioRuntimeProvider::default(),
    )
    .with_options(options)
    .build()
    {
        Ok(resolver) => {
            let samples = join_all(
                (0..samples_per_protocol)
                    .map(|_| tokio::time::timeout(timeout, resolver.lookup_ip(domain))),
            )
            .await;
            let mut addresses = Vec::new();
            let mut no_records = false;
            let mut errors = Vec::new();
            let mut response_codes = Vec::new();
            for sample in samples {
                match sample {
                    Ok(Ok(lookup)) => {
                        addresses.extend(lookup.iter());
                        response_codes.push("NoError".to_string());
                    }
                    Ok(Err(NetError::Dns(DnsError::NoRecordsFound(no_records_error)))) => {
                        no_records = true;
                        response_codes.push(no_records_error.response_code.to_string());
                    }
                    Ok(Err(NetError::Dns(DnsError::ResponseCode(code)))) => {
                        response_codes.push(code.to_string());
                        errors.push(format!("error response: {code}"));
                    }
                    Ok(Err(error)) => errors.push(error.to_string()),
                    Err(_) => errors.push("lookup timed out".to_string()),
                }
            }
            response_codes.sort();
            response_codes.dedup();
            let outcome = if !addresses.is_empty() {
                addresses.sort_unstable();
                addresses.dedup();
                DnsOutcome::Answer {
                    addresses: addresses.clone(),
                }
            } else if no_records {
                DnsOutcome::NoRecords
            } else {
                errors.sort();
                errors.dedup();
                DnsOutcome::Error {
                    message: errors.join("; "),
                }
            };
            let metadata = DnsResponseMetadata {
                response_codes,
                ipv4_count: addresses.iter().filter(|address| address.is_ipv4()).count() as u16,
                ipv6_count: addresses.iter().filter(|address| address.is_ipv6()).count() as u16,
            };
            (outcome, metadata)
        }
        Err(error) => (
            DnsOutcome::Error {
                message: error.to_string(),
            },
            DnsResponseMetadata::default(),
        ),
    };

    DnsObservation {
        provider: provider.to_string(),
        protocol,
        outcome,
        suspected_spoofing: false,
        metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::{mark_suspected_spoofing, suspicious_provider_count};
    use reports::probe::{DnsObservation, DnsOutcome, DnsProtocol, DnsResponseMetadata};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn normalized_answers_compare_independent_of_order() {
        let mut left = vec![
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        ];
        let right = vec![
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
        ];
        left.sort_unstable();
        assert_eq!(left, right);
    }

    #[test]
    fn protocol_rotation_is_not_mistaken_for_spoofing() {
        let answer = |last| DnsOutcome::Answer {
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, last))],
        };
        let observation = |protocol, outcome| DnsObservation {
            provider: "quad9".to_string(),
            protocol,
            outcome,
            suspected_spoofing: false,
            metadata: DnsResponseMetadata {
                response_codes: vec!["NoError".to_string()],
                ipv4_count: 1,
                ipv6_count: 0,
            },
        };
        let mut observations = vec![
            observation(DnsProtocol::Udp, answer(1)),
            observation(DnsProtocol::Tcp, answer(2)),
            observation(DnsProtocol::Doh, answer(1)),
            observation(DnsProtocol::Dot, answer(2)),
        ];
        mark_suspected_spoofing(&mut observations);
        assert!(observations.iter().all(|item| !item.suspected_spoofing));
    }

    #[test]
    fn different_addresses_with_the_same_metadata_are_not_suspicious() {
        let answer = |last| DnsOutcome::Answer {
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, last))],
        };
        let observation = |protocol, outcome| DnsObservation {
            provider: "quad9".to_string(),
            protocol,
            outcome,
            suspected_spoofing: false,
            metadata: DnsResponseMetadata {
                response_codes: vec!["NoError".to_string()],
                ipv4_count: 1,
                ipv6_count: 0,
            },
        };
        let mut observations = vec![
            observation(DnsProtocol::Udp, answer(99)),
            observation(DnsProtocol::Tcp, answer(1)),
            observation(DnsProtocol::Doh, answer(1)),
            observation(DnsProtocol::Dot, answer(1)),
        ];
        mark_suspected_spoofing(&mut observations);
        assert!(observations.iter().all(|item| !item.suspected_spoofing));
    }

    #[test]
    fn plaintext_metadata_difference_is_suspicious() {
        let observation = |protocol, ipv4_count| DnsObservation {
            provider: "quad9".to_string(),
            protocol,
            outcome: DnsOutcome::Answer {
                addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))],
            },
            suspected_spoofing: false,
            metadata: DnsResponseMetadata {
                response_codes: vec!["NoError".to_string()],
                ipv4_count,
                ipv6_count: 0,
            },
        };
        let mut observations = vec![
            observation(DnsProtocol::Udp, 2),
            observation(DnsProtocol::Tcp, 1),
            observation(DnsProtocol::Doh, 1),
            observation(DnsProtocol::Dot, 1),
        ];
        mark_suspected_spoofing(&mut observations);
        assert!(observations[0].suspected_spoofing);
        assert!(
            observations[1..]
                .iter()
                .all(|item| !item.suspected_spoofing)
        );
    }

    #[test]
    fn multiple_protocols_from_one_provider_count_as_one_vote() {
        let observation = |provider: &str, protocol| DnsObservation {
            provider: provider.to_string(),
            protocol,
            outcome: DnsOutcome::NoRecords,
            suspected_spoofing: true,
            metadata: DnsResponseMetadata::default(),
        };
        let observations = vec![
            observation("quad9", DnsProtocol::Udp),
            observation("quad9", DnsProtocol::Tcp),
            observation("yandex", DnsProtocol::Udp),
        ];
        assert_eq!(suspicious_provider_count(&observations[..2]), 1);
        assert_eq!(suspicious_provider_count(&observations), 2);
    }
}
