use super::rate_limit::ApiRateLimiter;
use crate::db::{WhitelistedEntry, check_whitelist, save_query};
use log::warn;
use querying::asn::AsnInfo;
use querying::geoip::IpInfo;
use querying::lists::NetworkRecord;
use querying::target::Target;
use querying::{Check, CheckError, CheckVerdict, Checker};
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::tokio::sync::RwLock;
use rocket_client_addr::ClientRealAddr;
use serde::Serialize;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

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
    pub reverse_lookup: Vec<String>,
    pub complaints: Vec<ComplaintDay>,
}

#[derive(Serialize)]
pub struct ComplaintDay {
    pub date: String,
    pub count: i64,
}

async fn load_complaints(
    db: &mut sqlx::PgConnection,
    target: &Target,
) -> Result<Vec<ComplaintDay>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT days.day::date::text AS "date!", COALESCE(reports.count, 0)::bigint AS "count!"
         FROM generate_series(current_date - 13, current_date, interval '1 day') AS days(day)
         LEFT JOIN (
             SELECT h.date::date AS day, COUNT(DISTINCT h.source_ip)::bigint AS count
             FROM human_reports h
             JOIN queries q ON q.id = h.id
             WHERE h.date >= current_date - 13
               AND h.date < current_date + 1
               AND h.works = false
               AND LOWER(q.query) = LOWER($1)
             GROUP BY h.date::date
         ) reports ON reports.day = days.day::date
         ORDER BY days.day"#,
        target.to_query()
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ComplaintDay {
            date: row.date,
            count: row.count,
        })
        .collect())
}

#[get("/check?<target>")]
pub async fn check(
    target: &str,
    checker: &State<Arc<RwLock<Checker>>>,
    addr: &ClientRealAddr,
    pool: &State<PgPool>,
    limiter: &State<Arc<ApiRateLimiter>>,
) -> Result<Json<ApiCheckResponse>, Status> {
    if !limiter.check(&addr.ip) {
        return Err(Status::TooManyRequests);
    }

    let target = Target::from(target.trim());
    let check = checker.read().await.check(target.clone()).await;

    let mut db = pool
        .acquire()
        .await
        .map_err(|_| Status::InternalServerError)?;

    let id: Option<String> = if let Ok(check) = &check {
        match save_query(&mut db, &target, check, addr, checker.read().await).await {
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
        check_whitelist(domain, &mut db)
            .await
            .map_err(|_| Status::InternalServerError)?
    } else {
        None
    };

    let complaints = match &check {
        Ok(_) => match load_complaints(&mut db, &target).await {
            Ok(days) => days,
            Err(e) => {
                warn!("api: failed to load complaints: {:?}", e);
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    };

    match check {
        Err(CheckError::NotFound) => Err(Status::NotFound),
        Ok(check) => Ok(Json(build_response(
            id, &target, check, whitelist, complaints,
        ))),
        Err(e) => {
            log::error!("api check failed {:?}", e);
            Err(Status::InternalServerError)
        }
    }
}

fn build_response(
    id: Option<String>,
    target: &Target,
    check: Check,
    whitelist: Option<WhitelistedEntry>,
    complaints: Vec<ComplaintDay>,
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
        reverse_lookup: check.reverse_lookup,
        blocked_subnets: check.rkn_subnets.iter().map(|n| n.to_string()).collect(),
        cdn_providers,
        geo: check.geo,
        asn_info: check.asn_info,
        whitelist,
        subnet_size: target.subnet_size(),
        complaints,
    }
}
