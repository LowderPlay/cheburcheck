use crate::db::{WhitelistedEntry, check_whitelist, save_query};
use crate::mqtt::{MqttPublisher, PublishError};
use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use log::warn;
use querying::asn::AsnInfo;
use querying::geoip::IpInfo;
use querying::lists::NetworkRecord;
use querying::target::Target;
use querying::{Check, CheckError, CheckVerdict, Checker};
use reports::probe::{
    Host, HostProbeResult, HostType, ProbeConfig, ProbeEvidence, ProbeResultEvent,
};
use rocket::State;
use rocket::http::Status;
use rocket::response::stream::{Event, EventStream};
use rocket::serde::json::Json;
use rocket::serde::json::serde_json::json;
use rocket::tokio::sync::RwLock;
use rocket::tokio::time;
use rocket_client_addr::ClientRealAddr;
use serde::Serialize;
use sqlx::postgres::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

pub type ApiRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

pub fn build_rate_limiter(per_minute: u32) -> ApiRateLimiter {
    RateLimiter::keyed(Quota::per_minute(
        NonZeroU32::new(per_minute).expect("rate limit must be > 0"),
    ))
}

#[derive(Serialize)]
pub struct ApiCheckResponse {
    pub id: Option<String>,
    pub target: String,
    pub target_type: String,
    pub blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rkn_domain: Option<String>,
    pub ips: Vec<String>,
    pub blocked_subnets: Vec<String>,
    pub cdn_providers: HashMap<String, Vec<NetworkRecord>>,
    pub geo: IpInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn_info: Option<AsnInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whitelist: Option<WhitelistedEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subnet_size: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct ProbeReporterInfo {
    pub region: Option<String>,
    pub provider: Option<String>,
    pub asn: Option<String>,
}

fn build_response(
    id: Option<String>,
    target: &Target,
    check: Check,
    whitelist: Option<WhitelistedEntry>,
) -> ApiCheckResponse {
    let (blocked, rkn_domain, cdn_providers) = match check.verdict {
        CheckVerdict::Blocked {
            rkn_domain,
            cdn_provider_subnets,
        } => {
            let providers: HashMap<String, Vec<NetworkRecord>> = cdn_provider_subnets
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().collect()))
                .collect();
            (true, rkn_domain, providers)
        }
        CheckVerdict::Clear => (false, None, HashMap::new()),
    };

    ApiCheckResponse {
        id,
        target: target.to_query(),
        target_type: target.readable_type().to_string(),
        blocked,
        rkn_domain,
        ips: check.ips.iter().map(|ip| ip.to_string()).collect(),
        blocked_subnets: check.rkn_subnets.iter().map(|n| n.to_string()).collect(),
        cdn_providers,
        geo: check.geo,
        asn_info: check.asn_info,
        whitelist,
        subnet_size: target.subnet_size(),
    }
}

#[get("/check?<target>")]
pub async fn check(
    target: &str,
    checker: &State<Arc<RwLock<Checker>>>,
    addr: &ClientRealAddr,
    pool: &State<PgPool>,
    limiter: &State<Arc<ApiRateLimiter>>,
) -> Result<Json<ApiCheckResponse>, Status> {
    if limiter.check_key(&addr.ip).is_err() {
        return Err(Status::TooManyRequests);
    }

    let target = Target::from(target.trim());
    let check = checker.read().await.check(target.clone()).await;

    let mut db = pool
        .acquire()
        .await
        .map_err(|_| Status::InternalServerError)?;

    let id: Option<String> = if let Ok(check) = &check {
        match save_query(&mut *db, &target, check, addr, checker.read().await).await {
            Ok(id) => Some(id.to_string()),
            Err(e) => {
                warn!("api: failed to save check: {:?}", e);
                None
            }
        }
    } else {
        None
    };

    let whitelist: Option<WhitelistedEntry> = if let Target::Domain(domain) = &target {
        check_whitelist(domain, &mut *db)
            .await
            .map_err(|_| Status::InternalServerError)?
    } else {
        None
    };

    match check {
        Err(CheckError::NotFound) => Err(Status::NotFound),
        Ok(check) => Ok(Json(build_response(id, &target, check, whitelist))),
        Err(e) => {
            log::error!("api check failed {:?}", e);
            Err(Status::InternalServerError)
        }
    }
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
        match error {
            PublishError::NotConfigured => Status::ServiceUnavailable,
            PublishError::Config(_)
            | PublishError::ConfigParse(_)
            | PublishError::Serialize(_)
            | PublishError::Subscribe(_)
            | PublishError::Publish(_) => Status::InternalServerError,
        }
    })?;

    mqtt.publish_probe_task(id, &domain)
        .await
        .map_err(|error| {
            warn!("api: failed to publish probe task for {id}: {error}");
            match error {
                PublishError::NotConfigured => Status::ServiceUnavailable,
                PublishError::Config(_)
                | PublishError::ConfigParse(_)
                | PublishError::Serialize(_)
                | PublishError::Subscribe(_)
                | PublishError::Publish(_) => Status::InternalServerError,
            }
        })?;

    let timeout = mqtt.task_timeout();
    let online_probes = mqtt.online_probe_count().await;
    let probe_config = mqtt.probe_config();
    let pool = pool.inner().clone();
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
                yield Event::data(json!({
                    "id": id,
                    "status": "done",
                    "response_count": responded_probes.len(),
                    "online_probes": online_probes,
                }).to_string()).event("done");
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
                            yield Event::data(build_probe_response(result, &probe_config, reporter_info).to_string()).event("result");
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
                    yield Event::data(json!({
                        "id": id,
                        "status": "done",
                        "response_count": responded_probes.len(),
                        "online_probes": online_probes,
                    }).to_string()).event("done");
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
) -> rocket::serde::json::Value {
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

fn is_strict_majority(total: usize, count: usize) -> bool {
    count > total / 2
}

#[get("/healthcheck")]
pub async fn healthcheck(checker: &State<Arc<RwLock<Checker>>>) -> (Status, String) {
    if checker.read().await.last_update().is_some() {
        (Status::Ok, "OK".to_string())
    } else {
        (Status::InternalServerError, "LOADING DATABASES".to_string())
    }
}

#[post("/feedback/<uuid>/<works>")]
pub async fn feedback(
    uuid: &str,
    works: bool,
    pool: &State<PgPool>,
    addr: &ClientRealAddr,
) -> Result<(), Status> {
    sqlx::query!(
        "INSERT INTO human_reports (id, source_ip, works) VALUES ($1, $2, $3)",
        Uuid::try_parse(uuid).map_err(|_| Status::BadRequest)?,
        addr.ip.to_string(),
        works
    )
    .execute(&**pool)
    .await
    .map_err(|_| Status::InternalServerError)?;

    Ok(())
}

#[derive(Serialize)]
pub struct ApiStatusResponse {
    domain_count: usize,
    v4_count: usize,
    last_update: Option<DateTime<Utc>>,
    version: &'static str,
}

#[get("/status")]
pub async fn get_system_status(checker: &State<Arc<RwLock<Checker>>>) -> Json<ApiStatusResponse> {
    let checker_ref = checker.read().await;
    Json(ApiStatusResponse {
        domain_count: checker_ref.total_domains().await,
        v4_count: checker_ref.total_v4s().await,
        last_update: checker_ref.last_update(),
        version: env!("CARGO_PKG_VERSION"),
    })
}
