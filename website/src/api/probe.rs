use super::rate_limit::ProbeRateLimiter;
use crate::mqtt::{MqttPublisher, PublishError};
use log::warn;
use querying::target::Target;
use reports::probe::{
    Host, HostProbeResult, HostType, ProbeConfig, ProbeEvidence, ProbeResultEvent,
    TcpTracerouteOutcome, TcpTracerouteResult,
};
use rocket::State;
use rocket::http::Status;
use rocket::response::stream::{Event, EventStream};
use rocket::serde::json::serde_json::Value;
use rocket::serde::json::serde_json::json;
use rocket::tokio::time;
use rocket_client_addr::ClientRealAddr;
use sqlx::postgres::PgPool;
use sqlx::types::Uuid;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;

#[derive(sqlx::FromRow)]
pub struct ProbeReporterInfo {
    pub region: Option<String>,
    pub provider: Option<String>,
    pub asn: Option<String>,
}

#[get("/probe/<id>")]
pub async fn probe_query(
    id: &str,
    addr: &ClientRealAddr,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
    limiter: &State<Arc<ProbeRateLimiter>>,
) -> Result<EventStream![Event], Status> {
    if !limiter.check(&addr.ip) {
        return Err(Status::TooManyRequests);
    }

    let id = Uuid::try_parse(id).map_err(|_| Status::BadRequest)?;
    let query: Option<(String, Vec<String>)> =
        sqlx::query_as("SELECT query, resolved_ips FROM queries WHERE id = $1")
            .bind(id)
            .fetch_optional(&**pool)
            .await
            .map_err(|_| Status::InternalServerError)?;

    let (query, resolved_ips) = query.ok_or(Status::NotFound)?;
    let target = Target::from(query.trim());
    let domain = match &target {
        Target::Domain(domain) => Some(domain.as_str()),
        Target::Ipv4(_) | Target::Ipv6(_) => None,
        Target::Ipv4Subnet(_) | Target::Ipv6Subnet(_) | Target::Asn(_) => {
            return Err(Status::BadRequest);
        }
    };
    let probe_config = mqtt.probe_config();
    if domain.is_none() && !probe_config.traceroute_enabled {
        return Err(Status::BadRequest);
    }
    let ip = resolved_ips
        .first()
        .and_then(|ip| ip.parse::<IpAddr>().ok())
        .ok_or(Status::BadRequest)?;
    if Target::is_bogon(ip) {
        return Err(Status::Forbidden);
    }

    let mut results = mqtt.subscribe_probe_results(id).await.map_err(|error| {
        warn!("api: failed to subscribe to probe results for {id}: {error}");
        publish_error_status(error)
    })?;

    mqtt.publish_probe_task(id, domain, ip)
        .await
        .map_err(|error| {
            warn!("api: failed to publish probe task for {id}: {error}");
            publish_error_status(error)
        })?;

    let timeout = mqtt.task_timeout();
    let online_probes = mqtt.online_probe_count().await;
    let pool = pool.inner().clone();
    let query_id = id;
    let id = id.to_string();
    Ok(EventStream! {
        let mut responded_probes = HashSet::new();
        let timeout = time::sleep(timeout);
        rocket::tokio::pin!(timeout);

        yield Event::data(json!({
            "id": id,
            "target": query,
            "online_probes": online_probes,
        }).to_string()).event("started");

        loop {
            if responded_probes.len() >= online_probes {
                yield done_event(&id, responded_probes.len(), online_probes);
                break;
            }

            rocket::tokio::select! {
                result = results.recv() => {
                    match result {
                        Ok(result) => {
                            responded_probes.insert(result.probe_id.clone());
                            let target_traceroute = result.target_traceroute.clone();
                            let control_traceroute = result.control_traceroute.clone();
                            let reporter_info = match fetch_probe_reporter_info(&result.probe_id, &pool).await {
                                Ok(info) => info,
                                Err(error) => {
                                    warn!(
                                        "api: failed to fetch reporter info for probe {}: {}",
                                        result.probe_id, error
                                    );
                                    None
                                }
                            };
                            let response = build_probe_response(result, &probe_config, reporter_info);
                            if let Err(error) = insert_probe_report(
                                query_id,
                                &response,
                                target_traceroute.as_ref(),
                                control_traceroute.as_ref(),
                                &pool,
                            ).await {
                                warn!("api: failed to save probe report for query {id}: {error}");
                            }
                            yield Event::data(response.to_string()).event("result");
                        }
                        Err(rocket::tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            continue;
                        }
                        Err(rocket::tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                _ = &mut timeout => {
                    yield done_event(&id, responded_probes.len(), online_probes);
                    break;
                }
            }
        }
    })
}

pub fn build_probe_response(
    raw: ProbeResultEvent,
    config: &ProbeConfig,
    reporter_info: Option<ProbeReporterInfo>,
) -> Value {
    let hosts: HashMap<&String, &Host> = config.hosts.iter().map(|h| (&h.id, h)).collect();
    let verdict = build_probe_verdict(
        &raw.host_results,
        config,
        raw.target_traceroute.as_ref(),
        raw.control_traceroute.as_ref(),
        raw.dns.as_ref(),
    );
    let target_hop =
        raw.target_traceroute
            .as_ref()
            .and_then(|traceroute| match &traceroute.result {
                TcpTracerouteOutcome::Rst { hop }
                | TcpTracerouteOutcome::Connected { hop }
                | TcpTracerouteOutcome::IcmpTimeExceeded { hop } => Some(*hop),
                TcpTracerouteOutcome::Timeout => None,
            });
    let region = reporter_info.as_ref().and_then(|info| info.region.as_ref());
    let provider = reporter_info
        .as_ref()
        .and_then(|info| info.provider.as_ref());
    let asn = reporter_info.as_ref().and_then(|info| info.asn.as_ref());
    let host_results = raw
        .host_results
        .into_iter()
        .filter_map(|result| {
            let host = hosts.get(&result.host_id)?;

            Some(json!({
                "host_id": result.host_id,
                "host": host.host_type,
                "probe_evidence": result.probe_evidence,
            }))
        })
        .collect::<Vec<_>>();

    json!({
        "job_id": raw.job_id,
        "probe_id": raw.probe_id,
        "region": region,
        "provider": provider,
        "asn": asn,
        "verdict": verdict,
        "host_results": host_results,
        "target_hop": target_hop,
        "dns": raw.dns,
    })
}

async fn insert_probe_report(
    query_id: Uuid,
    response: &Value,
    target_traceroute: Option<&TcpTracerouteResult>,
    control_traceroute: Option<&TcpTracerouteResult>,
    pool: &PgPool,
) -> Result<(), sqlx::Error> {
    let probe_id = response
        .get("probe_id")
        .and_then(Value::as_str)
        .and_then(|probe_id| probe_id.parse::<i32>().ok());
    let verdict = response
        .get("verdict")
        .and_then(Value::as_str)
        .unwrap_or("uncertain");
    let (target_hop_count, target_trace_result) = traceroute_columns(target_traceroute);
    let (control_hop_count, control_trace_result) = traceroute_columns(control_traceroute);

    let Some(probe_id) = probe_id else {
        warn!("api: ignoring probe report with non-numeric probe_id");
        return Ok(());
    };

    sqlx::query(
        r#"
        INSERT INTO probe_reports (
            query_id,
            probe_id,
            verdict,
            result,
            target_hop_count,
            target_trace_result,
            control_hop_count,
            control_trace_result
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (query_id, probe_id)
        DO UPDATE SET
            date = NOW(),
            verdict = EXCLUDED.verdict,
            result = EXCLUDED.result,
            target_hop_count = EXCLUDED.target_hop_count,
            target_trace_result = EXCLUDED.target_trace_result,
            control_hop_count = EXCLUDED.control_hop_count,
            control_trace_result = EXCLUDED.control_trace_result
        "#,
    )
    .bind(query_id)
    .bind(probe_id)
    .bind(verdict)
    .bind(response)
    .bind(target_hop_count)
    .bind(target_trace_result)
    .bind(control_hop_count)
    .bind(control_trace_result)
    .execute(pool)
    .await?;

    Ok(())
}

fn traceroute_columns(
    traceroute: Option<&TcpTracerouteResult>,
) -> (Option<i16>, Option<&'static str>) {
    let Some(traceroute) = traceroute else {
        return (None, None);
    };
    let (hop, result) = match &traceroute.result {
        TcpTracerouteOutcome::Rst { hop } => (Some(*hop), "Rst"),
        TcpTracerouteOutcome::Connected { hop } => (Some(*hop), "Connected"),
        TcpTracerouteOutcome::IcmpTimeExceeded { hop } => (Some(*hop), "IcmpTimeExceeded"),
        TcpTracerouteOutcome::Timeout => (None, "Timeout"),
    };
    (hop.map(i16::from), Some(result))
}

async fn fetch_probe_reporter_info(
    probe_id: &str,
    pool: &PgPool,
) -> Result<Option<ProbeReporterInfo>, sqlx::Error> {
    sqlx::query_as::<_, ProbeReporterInfo>(
        "SELECT region, provider, asn FROM reporters WHERE id = $1 LIMIT 1",
    )
    .bind(probe_id.parse::<i32>().unwrap_or(-1))
    .fetch_optional(pool)
    .await
}

fn build_probe_verdict(
    results: &[HostProbeResult],
    config: &ProbeConfig,
    target_traceroute: Option<&TcpTracerouteResult>,
    control_traceroute: Option<&TcpTracerouteResult>,
    dns: Option<&reports::probe::DnsProbeResult>,
) -> &'static str {
    if dns.is_some_and(|result| result.spoofing_detected) {
        return "dns_spoofing";
    }
    if let (
        Some(TcpTracerouteResult {
            result: TcpTracerouteOutcome::IcmpTimeExceeded { hop: target_hop },
            ..
        }),
        Some(TcpTracerouteResult {
            result: TcpTracerouteOutcome::IcmpTimeExceeded { hop: control_hop },
            ..
        }),
    ) = (target_traceroute, control_traceroute)
        && target_hop < control_hop
    {
        return "tspu_block";
    }

    if results.is_empty() {
        return "ok";
    }

    let matched = results
        .iter()
        .filter_map(|result| {
            config
                .hosts
                .iter()
                .find(|host| host.id == result.host_id)
                .map(|host| (host, &result.probe_evidence))
        })
        .collect::<Vec<_>>();

    if matched.is_empty() {
        return "uncertain";
    }

    if is_strict_majority(
        matched.len(),
        matched
            .iter()
            .filter(|(_, evidence)| matches!(evidence, ProbeEvidence::ClientHello))
            .count(),
    ) {
        return "sni_block";
    }

    if is_strict_majority(
        matched.len(),
        matched
            .iter()
            .filter(|(_, evidence)| matches!(evidence, ProbeEvidence::Good))
            .count(),
    ) {
        return "whitelist";
    }

    let blacklist = matched
        .iter()
        .filter(|(host, _)| matches!(host.host_type, HostType::Blacklist))
        .collect::<Vec<_>>();
    let whitelist = matched
        .iter()
        .filter(|(host, _)| matches!(host.host_type, HostType::Whitelist))
        .collect::<Vec<_>>();

    let most_blacklist_timed_out = !blacklist.is_empty()
        && is_strict_majority(
            blacklist.len(),
            blacklist
                .iter()
                .filter(|(_, evidence)| matches!(evidence, ProbeEvidence::DataTimeout { .. }))
                .count(),
        );
    let most_whitelist_good = !whitelist.is_empty()
        && is_strict_majority(
            whitelist.len(),
            whitelist
                .iter()
                .filter(|(_, evidence)| matches!(evidence, ProbeEvidence::Good))
                .count(),
        );

    if most_blacklist_timed_out && most_whitelist_good {
        "ok"
    } else {
        "uncertain"
    }
}

fn publish_error_status(error: PublishError) -> Status {
    match error {
        PublishError::NotConfigured => Status::ServiceUnavailable,
        PublishError::Config(_)
        | PublishError::ConfigParse(_)
        | PublishError::Serialize(_)
        | PublishError::Subscribe(_)
        | PublishError::Publish(_) => Status::InternalServerError,
    }
}

fn done_event(id: &str, response_count: usize, online_probes: usize) -> Event {
    Event::data(
        json!({
            "id": id,
            "status": "done",
            "response_count": response_count,
            "online_probes": online_probes,
        })
        .to_string(),
    )
    .event("done")
}

fn is_strict_majority(total: usize, count: usize) -> bool {
    count > total / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icmp_trace(hop: u8) -> TcpTracerouteResult {
        TcpTracerouteResult {
            target: "192.0.2.1".parse().unwrap(),
            result: TcpTracerouteOutcome::IcmpTimeExceeded { hop },
        }
    }

    #[test]
    fn tspu_block_requires_an_earlier_target_icmp_hop() {
        let target = icmp_trace(3);
        let control = icmp_trace(5);
        assert_eq!(
            build_probe_verdict(&[], &empty_config(), Some(&target), Some(&control), None),
            "tspu_block"
        );

        let target = icmp_trace(5);
        assert_eq!(
            build_probe_verdict(&[], &empty_config(), Some(&target), Some(&control), None),
            "ok"
        );
    }

    #[test]
    fn tspu_block_requires_two_icmp_outcomes() {
        let target = TcpTracerouteResult {
            target: "192.0.2.1".parse().unwrap(),
            result: TcpTracerouteOutcome::Connected { hop: 2 },
        };
        let control = icmp_trace(5);
        assert_eq!(
            build_probe_verdict(&[], &empty_config(), Some(&target), Some(&control), None),
            "ok"
        );
    }

    fn empty_config() -> ProbeConfig {
        ProbeConfig {
            version: String::new(),
            task_timeout_ms: 0,
            published_at: String::new(),
            hosts: vec![],
            traceroute_enabled: true,
            control_hosts: vec![],
            dns_samples_per_protocol: reports::probe::default_dns_samples_per_protocol(),
            dns_spoofing_provider_threshold:
                reports::probe::default_dns_spoofing_provider_threshold(),
        }
    }
}
