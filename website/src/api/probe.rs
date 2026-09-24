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
use rocket::tokio::{sync::Mutex, time};
use rocket_client_addr::ClientRealAddr;
use sqlx::postgres::PgPool;
use sqlx::types::Uuid;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_PROBE_RESPONSE_CACHE_SECONDS: u64 = 3 * 60 * 60;

#[derive(Clone)]
struct CachedProbeResponse {
    response: Value,
    target_traceroute: Option<TcpTracerouteResult>,
    duration_ms: u64,
    stored_at: Instant,
}

pub struct ProbeResponseCache {
    ttl: Duration,
    entries: Mutex<HashMap<(String, String), CachedProbeResponse>>,
}

impl ProbeResponseCache {
    pub fn from_env() -> Self {
        let seconds = std::env::var("PROBE_RESPONSE_CACHE_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_PROBE_RESPONSE_CACHE_SECONDS);
        Self::new(Duration::from_secs(seconds))
    }

    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    async fn get_all(
        &self,
        target: &str,
        probe_ids: &[String],
    ) -> Option<Vec<(String, CachedProbeResponse)>> {
        let mut entries = self.entries.lock().await;
        entries.retain(|_, entry| entry.stored_at.elapsed() < self.ttl);
        probe_ids
            .iter()
            .map(|probe_id| {
                let entry = entries.get(&(target.to_owned(), probe_id.clone()))?;
                Some((probe_id.clone(), entry.clone()))
            })
            .collect()
    }

    async fn invalidate_target(&self, target: &str) {
        self.entries
            .lock()
            .await
            .retain(|(cached_target, _), _| cached_target != target);
    }

    async fn insert(
        &self,
        target: &str,
        probe_id: &str,
        response: Value,
        target_traceroute: Option<TcpTracerouteResult>,
        duration_ms: u64,
    ) {
        if self.ttl.is_zero() {
            return;
        }
        let mut entries = self.entries.lock().await;
        entries.retain(|_, entry| entry.stored_at.elapsed() < self.ttl);
        entries.insert(
            (target.to_owned(), probe_id.to_owned()),
            CachedProbeResponse {
                response,
                target_traceroute,
                duration_ms,
                stored_at: Instant::now(),
            },
        );
    }
}

#[derive(sqlx::FromRow)]
pub struct ProbeReporterInfo {
    pub region: Option<String>,
    pub provider: Option<String>,
    pub asn: Option<String>,
    pub disable_traceroutes: bool,
    pub cdn_unblocked: bool,
}

#[get("/probe/<id>?<token>")]
pub async fn probe_query(
    id: &str,
    token: Option<&str>,
    addr: &ClientRealAddr,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
    limiter: &State<Arc<ProbeRateLimiter>>,
    cache: &State<Arc<ProbeResponseCache>>,
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
    let is_ip_target = domain.is_none();
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

    let (target_probe, eligible_probes) = load_probe_targets(pool, token).await?;
    let active_probes = if target_probe.is_some() {
        eligible_probes
    } else {
        mqtt.online_probe_ids(&eligible_probes).await
    };
    let cache_target = target.to_query().to_ascii_lowercase();
    let cached = if target_probe.is_some() {
        None
    } else {
        cache.get_all(&cache_target, &active_probes).await
    };
    let (cached, expected_probes) = match cached {
        Some(cached) => (cached, HashSet::new()),
        None => {
            if target_probe.is_none() {
                cache.invalidate_target(&cache_target).await;
            }
            (
                Vec::new(),
                active_probes.into_iter().collect::<HashSet<_>>(),
            )
        }
    };
    let online_probes = cached.len() + expected_probes.len();

    let results = if expected_probes.is_empty() {
        None
    } else {
        let receiver = mqtt.subscribe_probe_results(id).await.map_err(|error| {
            warn!("api: failed to subscribe to probe results for {id}: {error}");
            publish_error_status(error)
        })?;
        let published_at = Instant::now();
        mqtt.publish_probe_task(id, domain, ip, target_probe.as_deref())
            .await
            .map_err(|error| {
                warn!("api: failed to publish probe task for {id}: {error}");
                publish_error_status(error)
            })?;
        Some((receiver, published_at))
    };

    let timeout = mqtt.task_timeout();
    let pool = pool.inner().clone();
    let cache = cache.inner().clone();
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

        for (probe_id, mut entry) in cached {
            entry.response["job_id"] = json!(id);
            if let Err(error) = insert_probe_report(
                query_id,
                &entry.response,
                entry.target_traceroute.as_ref(),
                entry.duration_ms,
                &pool,
            ).await {
                warn!("api: failed to save cached probe report for query {id}: {error}");
            }
            responded_probes.insert(probe_id);
            yield Event::data(entry.response.to_string()).event("result");
        }

        if let Some((mut results, published_at)) = results {
            loop {
                if responded_probes.len() >= online_probes {
                    yield done_event(&id, responded_probes.len(), online_probes);
                    break;
                }

                rocket::tokio::select! {
                    result = results.recv() => {
                        match result {
                            Ok(mut result) => {
                                if !expected_probes.contains(&result.probe_id) {
                                    continue;
                                }
                                let duration_ms = u64::try_from(published_at.elapsed().as_millis())
                                    .unwrap_or(u64::MAX);
                                let reporter_info = match fetch_probe_reporter_info(&result.probe_id, &pool).await {
                                    Ok(info) => info,
                                    Err(error) => {
                                        warn!(
                                            "api: failed to fetch reporter info for probe {}: {}",
                                            result.probe_id, error
                                        );
                                        continue;
                                    }
                                };
                                let Some(reporter_info) = reporter_info else {
                                    continue;
                                };
                                let result_probe_id = result.probe_id.clone();
                                responded_probes.insert(result_probe_id.clone());
                                filter_probe_traceroute(&mut result, reporter_info.disable_traceroutes);
                                let target_traceroute = result.target_traceroute.clone();
                                let response = build_probe_response(
                                    result,
                                    &probe_config,
                                    Some(reporter_info),
                                    is_ip_target,
                                );
                                if let Err(error) = insert_probe_report(
                                    query_id,
                                    &response,
                                    target_traceroute.as_ref(),
                                    duration_ms,
                                    &pool,
                                ).await {
                                    warn!("api: failed to save probe report for query {id}: {error}");
                                }
                                if target_probe.is_none() {
                                    cache.insert(
                                        &cache_target,
                                        &result_probe_id,
                                        response.clone(),
                                        target_traceroute,
                                        duration_ms,
                                    ).await;
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
        } else {
            yield done_event(&id, responded_probes.len(), online_probes);
        }
    })
}

async fn load_probe_targets(
    pool: &PgPool,
    token: Option<&str>,
) -> Result<(Option<String>, Vec<String>), Status> {
    if token.is_some_and(str::is_empty) {
        return Err(Status::BadRequest);
    }
    if let Some(token) = token {
        let probe_id =
            sqlx::query_scalar::<_, i32>("SELECT id FROM reporters WHERE token = $1 LIMIT 1")
                .bind(token)
                .fetch_optional(pool)
                .await
                .map_err(|error| {
                    warn!("api: failed to resolve targeted probe: {error}");
                    Status::InternalServerError
                })?
                .ok_or(Status::NotFound)?
                .to_string();

        return Ok((Some(probe_id.clone()), vec![probe_id]));
    }

    let probe_ids =
        sqlx::query_scalar::<_, i32>("SELECT id FROM reporters WHERE hidden = FALSE ORDER BY id")
            .fetch_all(pool)
            .await
            .map_err(|error| {
                warn!("api: failed to load global probe recipients: {error}");
                Status::InternalServerError
            })?
            .into_iter()
            .map(|id| id.to_string())
            .collect();

    Ok((None, probe_ids))
}

fn filter_probe_traceroute(result: &mut ProbeResultEvent, disable_traceroutes: bool) {
    if disable_traceroutes {
        result.target_traceroute = None;
    }
}

pub fn build_probe_response(
    raw: ProbeResultEvent,
    config: &ProbeConfig,
    reporter_info: Option<ProbeReporterInfo>,
    is_ip_target: bool,
) -> Value {
    let hosts: HashMap<&String, &Host> = config.hosts.iter().map(|h| (&h.id, h)).collect();
    let verdicts = build_probe_verdicts(
        &raw.host_results,
        config,
        raw.target_traceroute.as_ref(),
        raw.dpi_hop,
        raw.dns.as_ref(),
        is_ip_target,
        reporter_info
            .as_ref()
            .is_some_and(|info| info.cdn_unblocked),
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
        "cdn_unblocked": reporter_info.as_ref().is_some_and(|info| info.cdn_unblocked),
        "verdicts": verdicts,
        "host_results": host_results,
        "target_hop": target_hop,
        "dpi_hop": raw.dpi_hop,
        "dns": raw.dns,
    })
}

async fn insert_probe_report(
    query_id: Uuid,
    response: &Value,
    target_traceroute: Option<&TcpTracerouteResult>,
    duration_ms: u64,
    pool: &PgPool,
) -> Result<(), sqlx::Error> {
    let probe_id = response
        .get("probe_id")
        .and_then(Value::as_str)
        .and_then(|probe_id| probe_id.parse::<i32>().ok());
    let verdicts = response
        .get("verdicts")
        .and_then(Value::as_array)
        .map(|verdicts| {
            verdicts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["uncertain"]);
    let (target_hop_count, target_trace_result) = traceroute_columns(target_traceroute);
    let duration_ms = i64::try_from(duration_ms).unwrap_or(i64::MAX);

    let Some(probe_id) = probe_id else {
        warn!("api: ignoring probe report with non-numeric probe_id");
        return Ok(());
    };

    sqlx::query(
        r#"
        INSERT INTO probe_reports (
            query_id,
            probe_id,
            verdicts,
            result,
            target_hop_count,
            target_trace_result,
            duration_ms
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (query_id, probe_id)
        DO UPDATE SET
            date = NOW(),
            verdicts = EXCLUDED.verdicts,
            result = EXCLUDED.result,
            target_hop_count = EXCLUDED.target_hop_count,
            target_trace_result = EXCLUDED.target_trace_result,
            duration_ms = EXCLUDED.duration_ms
        "#,
    )
    .bind(query_id)
    .bind(probe_id)
    .bind(verdicts)
    .bind(response)
    .bind(target_hop_count)
    .bind(target_trace_result)
    .bind(duration_ms)
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
        "SELECT region, provider, asn, disable_traceroutes, cdn_unblocked FROM reporters WHERE id = $1 LIMIT 1",
    )
    .bind(probe_id.parse::<i32>().unwrap_or(-1))
    .fetch_optional(pool)
    .await
}

fn build_probe_verdicts(
    results: &[HostProbeResult],
    config: &ProbeConfig,
    target_traceroute: Option<&TcpTracerouteResult>,
    dpi_hop: Option<u8>,
    dns: Option<&reports::probe::DnsProbeResult>,
    is_ip_target: bool,
    cdn_unblocked: bool,
) -> Vec<&'static str> {
    if results.is_empty() && target_traceroute.is_none() && dns.is_none() {
        return vec!["uncertain"];
    }

    let tspu_block = dpi_hop.is_some()
        && target_traceroute
            .is_some_and(|traceroute| matches!(&traceroute.result, TcpTracerouteOutcome::Timeout));

    if is_ip_target {
        return if tspu_block {
            vec!["tspu_block"]
        } else if target_traceroute
            .is_some_and(|traceroute| !matches!(&traceroute.result, TcpTracerouteOutcome::Timeout))
        {
            vec!["ok"]
        } else {
            vec!["uncertain"]
        };
    }

    let host_verdict = build_host_verdict(results, config, cdn_unblocked);
    let dns_spoofing = dns.is_some_and(|result| result.spoofing_detected);

    let mut verdicts = [
        tspu_block.then_some("tspu_block"),
        (!matches!(host_verdict, "ok" | "uncertain")).then_some(host_verdict),
        dns_spoofing.then_some("dns_spoofing"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    if verdicts.is_empty() {
        verdicts.push(host_verdict);
    }

    verdicts
}

fn build_host_verdict(
    results: &[HostProbeResult],
    config: &ProbeConfig,
    cdn_unblocked: bool,
) -> &'static str {
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

    if results.is_empty() {
        "ok"
    } else if matched.is_empty() {
        "uncertain"
    } else if is_strict_majority(
        matched
            .iter()
            .filter(|(h, _)| matches!(h.host_type, HostType::Whitelist))
            .count(),
        matched
            .iter()
            .filter(|(h, evidence)| {
                matches!(h.host_type, HostType::Whitelist)
                    && matches!(evidence, ProbeEvidence::ClientHello)
            })
            .count(),
    ) {
        "sni_block"
    } else if !cdn_unblocked
        && is_strict_majority(
            matched
                .iter()
                .filter(|(h, _)| matches!(h.host_type, HostType::Blacklist))
                .count(),
            matched
                .iter()
                .filter(|(host, evidence)| {
                    matches!(host.host_type, HostType::Blacklist)
                        && !matches!(evidence, ProbeEvidence::DataTimeout { .. })
                })
                .count(),
        )
    {
        "whitelist"
    } else {
        let blacklist = matched
            .iter()
            .filter(|(host, _)| matches!(host.host_type, HostType::Blacklist))
            .collect::<Vec<_>>();
        let whitelist = matched
            .iter()
            .filter(|(host, _)| matches!(host.host_type, HostType::Whitelist))
            .collect::<Vec<_>>();

        let most_blacklist_matches_baseline = !blacklist.is_empty()
            && is_strict_majority(
                blacklist.len(),
                blacklist
                    .iter()
                    .filter(|(_, evidence)| {
                        if cdn_unblocked {
                            matches!(evidence, ProbeEvidence::Good)
                        } else {
                            matches!(evidence, ProbeEvidence::DataTimeout { .. })
                        }
                    })
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

        if most_blacklist_matches_baseline && most_whitelist_good {
            "ok"
        } else {
            "uncertain"
        }
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

    #[test]
    fn cache_is_scoped_by_target_and_probe_and_expires() {
        rocket::tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let cache = ProbeResponseCache::new(Duration::from_secs(10));
                cache
                    .insert("example.org", "42", json!({"job_id": "old"}), None, 123)
                    .await;
                assert_eq!(
                    cache.get_all("example.org", &["42".into()]).await.unwrap()[0]
                        .1
                        .duration_ms,
                    123
                );
                assert!(cache.get_all("other.org", &["42".into()]).await.is_none());
                assert!(cache.get_all("example.org", &["43".into()]).await.is_none());
                assert!(
                    cache
                        .get_all("example.org", &["42".into(), "43".into()])
                        .await
                        .is_none()
                );
                cache
                    .insert("other.org", "42", json!({"job_id": "other"}), None, 456)
                    .await;
                cache.invalidate_target("other.org").await;
                assert!(cache.get_all("other.org", &["42".into()]).await.is_none());
                assert!(cache.get_all("example.org", &["42".into()]).await.is_some());

                cache
                    .entries
                    .lock()
                    .await
                    .get_mut(&("example.org".into(), "42".into()))
                    .unwrap()
                    .stored_at = Instant::now() - Duration::from_secs(11);
                assert!(cache.get_all("example.org", &["42".into()]).await.is_none());
            });
    }

    #[test]
    fn zero_ttl_disables_cache() {
        rocket::tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let cache = ProbeResponseCache::new(Duration::ZERO);
                cache.insert("example.org", "42", json!({}), None, 0).await;
                assert!(cache.get_all("example.org", &["42".into()]).await.is_none());
                assert!(cache.entries.lock().await.is_empty());
            });
    }

    #[test]
    fn disabled_traceroutes_are_not_persisted_or_used_in_public_verdicts() {
        for disabled in [false, true] {
            for outcome in [
                TcpTracerouteOutcome::Timeout,
                TcpTracerouteOutcome::Connected { hop: 5 },
            ] {
                let mut result = ProbeResultEvent {
                    job_id: "job".to_string(),
                    probe_id: "42".to_string(),
                    host_results: vec![HostProbeResult {
                        host_id: "test".to_string(),
                        probe_evidence: ProbeEvidence::Good,
                    }],
                    target_traceroute: Some(TcpTracerouteResult {
                        target: "192.0.2.1".parse().unwrap(),
                        result: outcome,
                    }),
                    dpi_hop: Some(4),
                    dns: Some(reports::probe::DnsProbeResult {
                        spoofing_detected: true,
                        suspicious_provider_count: 2,
                        verdict_threshold: 2,
                        samples_per_protocol: 3,
                        observations: vec![],
                    }),
                };
                filter_probe_traceroute(&mut result, disabled);
                assert_eq!(result.host_results.len(), 1);
                assert!(result.dns.as_ref().unwrap().spoofing_detected);
                let columns = traceroute_columns(result.target_traceroute.as_ref());
                let response = build_probe_response(result, &empty_config(), None, true);
                assert!(response.get("duration_ms").is_none());
                if disabled {
                    assert_eq!(columns, (None, None));
                    assert_eq!(response["target_hop"], Value::Null);
                    assert_eq!(response["verdicts"], json!(["uncertain"]));
                } else {
                    assert!(columns.1.is_some());
                    if columns.1 == Some("Timeout") {
                        assert_eq!(response["verdicts"], json!(["tspu_block"]));
                    } else {
                        assert_eq!(response["target_hop"], json!(5));
                    }
                }
            }
        }
    }

    #[test]
    fn no_checks_returns_uncertain() {
        assert_eq!(
            build_probe_verdicts(&[], &empty_config(), None, None, None, false, false),
            vec!["uncertain"]
        );
    }

    #[test]
    fn ip_target_only_reports_tspu_block_with_dpi_hop_and_timeout() {
        let timeout = TcpTracerouteResult {
            target: "192.0.2.1".parse().unwrap(),
            result: TcpTracerouteOutcome::Timeout,
        };
        assert_eq!(
            build_probe_verdicts(
                &[],
                &empty_config(),
                Some(&timeout),
                Some(4),
                None,
                true,
                false
            ),
            vec!["tspu_block"]
        );
        assert_eq!(
            build_probe_verdicts(
                &[],
                &empty_config(),
                Some(&timeout),
                None,
                None,
                true,
                false
            ),
            vec!["uncertain"]
        );
    }

    #[test]
    fn reachable_ip_target_is_ok() {
        for result in [
            TcpTracerouteOutcome::IcmpTimeExceeded { hop: 5 },
            TcpTracerouteOutcome::Connected { hop: 5 },
            TcpTracerouteOutcome::Rst { hop: 5 },
        ] {
            let target = TcpTracerouteResult {
                target: "192.0.2.1".parse().unwrap(),
                result,
            };
            assert_eq!(
                build_probe_verdicts(
                    &[],
                    &empty_config(),
                    Some(&target),
                    Some(4),
                    None,
                    true,
                    false
                ),
                vec!["ok"]
            );
        }
    }

    #[test]
    fn reports_sni_block_and_dns_spoofing_together() {
        let mut config = empty_config();
        config.hosts.push(Host {
            id: "test".to_string(),
            host: "192.0.2.1".to_string(),
            host_type: HostType::Whitelist,
            file_path: String::new(),
            timeout_sec: 1,
            min_data: 1,
        });
        let results = vec![HostProbeResult {
            host_id: "test".to_string(),
            probe_evidence: ProbeEvidence::ClientHello,
        }];
        let dns = reports::probe::DnsProbeResult {
            spoofing_detected: true,
            suspicious_provider_count: 2,
            verdict_threshold: 2,
            samples_per_protocol: 3,
            observations: vec![],
        };

        assert_eq!(
            build_probe_verdicts(&results, &config, None, None, Some(&dns), false, false),
            vec!["sni_block", "dns_spoofing"]
        );
    }

    #[test]
    fn reports_whitelist_when_blacklist_host_returns_data() {
        let mut config = empty_config();
        config.hosts.push(Host {
            id: "test".to_string(),
            host: "192.0.2.1".to_string(),
            host_type: HostType::Blacklist,
            file_path: String::new(),
            timeout_sec: 1,
            min_data: 1,
        });
        let results = vec![HostProbeResult {
            host_id: "test".to_string(),
            probe_evidence: ProbeEvidence::ClientHello,
        }];

        assert_eq!(
            build_probe_verdicts(&results, &config, None, None, None, false, false),
            vec!["whitelist"]
        );
    }

    #[test]
    fn accessible_cdn_is_not_a_whitelist_exception_for_unblocked_reporter() {
        let mut config = empty_config();
        for (id, host_type) in [
            ("foreign-cdn", HostType::Blacklist),
            ("control", HostType::Whitelist),
        ] {
            config.hosts.push(Host {
                id: id.to_string(),
                host: "192.0.2.1".to_string(),
                host_type,
                file_path: String::new(),
                timeout_sec: 1,
                min_data: 1,
            });
        }
        let results = ["foreign-cdn", "control"]
            .into_iter()
            .map(|host_id| HostProbeResult {
                host_id: host_id.to_string(),
                probe_evidence: ProbeEvidence::Good,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            build_probe_verdicts(&results, &config, None, None, None, false, false),
            vec!["whitelist"]
        );
        assert_eq!(
            build_probe_verdicts(&results, &config, None, None, None, false, true),
            vec!["ok"]
        );

        let response = build_probe_response(
            ProbeResultEvent {
                job_id: "job".to_string(),
                probe_id: "42".to_string(),
                host_results: results,
                target_traceroute: None,
                dpi_hop: None,
                dns: None,
            },
            &config,
            Some(ProbeReporterInfo {
                region: None,
                provider: None,
                asn: None,
                disable_traceroutes: false,
                cdn_unblocked: true,
            }),
            false,
        );
        assert_eq!(response["cdn_unblocked"], json!(true));
        assert_eq!(response["verdicts"], json!(["ok"]));
    }

    #[test]
    fn unblocked_reporter_still_detects_sni_block() {
        let mut config = empty_config();
        config.hosts.push(Host {
            id: "control".to_string(),
            host: "192.0.2.1".to_string(),
            host_type: HostType::Whitelist,
            file_path: String::new(),
            timeout_sec: 1,
            min_data: 1,
        });
        let results = vec![HostProbeResult {
            host_id: "control".to_string(),
            probe_evidence: ProbeEvidence::ClientHello,
        }];

        assert_eq!(
            build_probe_verdicts(&results, &config, None, None, None, false, true),
            vec!["sni_block"]
        );
    }

    fn empty_config() -> ProbeConfig {
        ProbeConfig {
            version: String::new(),
            task_timeout_ms: 0,
            published_at: String::new(),
            hosts: vec![],
            traceroute_enabled: true,
            dns_samples_per_protocol: reports::probe::default_dns_samples_per_protocol(),
            dns_spoofing_provider_threshold:
                reports::probe::default_dns_spoofing_provider_threshold(),
            dpi_probe: None,
        }
    }
}
