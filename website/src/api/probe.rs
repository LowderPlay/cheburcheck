use super::rate_limit::ApiRateLimiter;
use crate::mqtt::{MqttPublisher, PublishError};
use log::warn;
use querying::target::Target;
use reports::probe::{
    Host, HostProbeResult, HostType, ProbeConfig, ProbeEvidence, ProbeResultEvent,
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
    limiter: &State<Arc<ApiRateLimiter>>,
) -> Result<EventStream![Event], Status> {
    if limiter.check_key(&addr.ip).is_err() {
        return Err(Status::TooManyRequests);
    }

    let id = Uuid::try_parse(id).map_err(|_| Status::BadRequest)?;
    let query: Option<String> = sqlx::query_scalar("SELECT query FROM queries WHERE id = $1")
        .bind(id)
        .fetch_optional(&**pool)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let query = query.ok_or(Status::NotFound)?;
    let Target::Domain(domain) = Target::from(query.trim()) else {
        return Err(Status::BadRequest);
    };

    let mut results = mqtt.subscribe_probe_results(id).await.map_err(|error| {
        warn!("api: failed to subscribe to probe results for {id}: {error}");
        publish_error_status(error)
    })?;

    mqtt.publish_probe_task(id, &domain)
        .await
        .map_err(|error| {
            warn!("api: failed to publish probe task for {id}: {error}");
            publish_error_status(error)
        })?;

    let timeout = mqtt.task_timeout();
    let online_probes = mqtt.online_probe_count().await;
    let probe_config = mqtt.probe_config();
    let pool = pool.inner().clone();
    let query_id = id;
    let id = id.to_string();
    Ok(EventStream! {
        let mut responded_probes = HashSet::new();
        let timeout = time::sleep(timeout);
        rocket::tokio::pin!(timeout);

        yield Event::data(json!({
            "id": id,
            "target": domain,
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
                            if let Err(error) = insert_probe_report(query_id, &response, &pool).await {
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
    let verdict = build_probe_verdict(&raw.host_results, config);
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
    })
}

async fn insert_probe_report(
    query_id: Uuid,
    response: &Value,
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

    let Some(probe_id) = probe_id else {
        warn!("api: ignoring probe report with non-numeric probe_id");
        return Ok(());
    };

    sqlx::query(
        r#"
        INSERT INTO probe_reports (query_id, probe_id, verdict, result)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (query_id, probe_id)
        DO UPDATE SET
            date = NOW(),
            verdict = EXCLUDED.verdict,
            result = EXCLUDED.result
        "#,
    )
    .bind(query_id)
    .bind(probe_id)
    .bind(verdict)
    .bind(response)
    .execute(pool)
    .await?;

    Ok(())
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

fn build_probe_verdict(results: &[HostProbeResult], config: &ProbeConfig) -> &'static str {
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
