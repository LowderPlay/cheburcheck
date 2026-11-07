use super::rate_limit::ApiRateLimiter;
use crate::analytics::Analytics;
use crate::check_context::CheckContext;
use log::warn;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::{Json, Value, serde_json};
use rocket::serde::{Serialize, json};
use rocket::response::status::Created;
use rocket_client_addr::ClientRealAddr;
use sqlx::PgPool;
use querying::Checker;
use rocket::tokio::sync::RwLock;
use std::time::Duration;
use std::sync::Arc;
use uuid::Uuid;

const SHARE_LIFETIME_DAYS: i32 = 30;
const MAX_SHARE_SIZE_BYTES: usize = 256 * 1024;
const SHARE_CLEANUP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct CreatedShare {
    id: String,
    created_at: String,
    expires_at: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct SharedResult {
    id: String,
    created_at: String,
    expires_at: String,
    snapshot: Value,
}

#[derive(rocket::serde::Deserialize)]
#[serde(crate = "rocket::serde", deny_unknown_fields)]
pub struct CreateShareRequest {
    id: String,
}

#[post("/shares", format = "json", data = "<request>")]
pub async fn create_share(
    request: Json<CreateShareRequest>,
    pool: &State<PgPool>,
    analytics: &State<Analytics>,
    contexts: &State<CheckContext>,
    checker: &State<Arc<RwLock<Checker>>>,
    limiter: &State<std::sync::Arc<ApiRateLimiter>>,
    addr: &ClientRealAddr,
) -> Result<Created<json::Json<CreatedShare>>, Status> {
    if !limiter.check(&addr.ip) {
        return Err(Status::TooManyRequests);
    }

    let context = contexts.verify(&request.id)?;
    let target = analytics
        .query_target(context.query_id)
        .await
        .map_err(|error| {
            warn!("api: failed to load query {} for sharing: {error}", context.query_id);
            Status::InternalServerError
        })?
        .filter(|target| target == &context.target)
        .ok_or(Status::NotFound)?;
    let result = super::build_shared_check_result(
        &target,
        checker,
        pool.inner(),
        analytics,
    )
    .await?;
    let probes = analytics.shared_probes(context.query_id).await.map_err(|error| {
        warn!("api: failed to load probes for shared check {}: {error}", context.query_id);
        Status::InternalServerError
    })?;
    let probe_status = serde_json::json!({
        "online_probes": probes.len(),
        "response_count": probes.len(),
        "status": "done"
    });
    let snapshot = serde_json::json!({
        "version": 1,
        "result": result,
        "probes": probes,
        "probe_status": probe_status,
    });

    let snapshot_json = serde_json::to_string(&snapshot).map_err(|error| {
        warn!("api: failed to serialize shared result: {error}");
        Status::BadRequest
    })?;
    if snapshot_json.len() > MAX_SHARE_SIZE_BYTES {
        return Err(Status::PayloadTooLarge);
    }

    let id = Uuid::new_v4();
    let row = sqlx::query_as::<_, (String, String, String)>(
        "INSERT INTO shared_results (id, query_id, snapshot, expires_at)
         VALUES ($1, $2, $3::jsonb, NOW() + make_interval(days => $4::int))
         ON CONFLICT (query_id) DO UPDATE
         SET snapshot = EXCLUDED.snapshot,
             created_at = NOW(),
             expires_at = EXCLUDED.expires_at
         RETURNING
             id::text,
             to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),
             to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')",
    )
    .bind(id)
    .bind(context.query_id)
    .bind(snapshot_json)
    .bind(SHARE_LIFETIME_DAYS)
    .fetch_one(pool.inner())
    .await
    .map_err(|error| {
        warn!("api: failed to save shared result: {error}");
        Status::InternalServerError
    })?;

    Ok(Created::new(format!("/api/v1/shares/{}", row.0)).body(json::Json(
        CreatedShare {
            id: row.0,
            created_at: row.1,
            expires_at: row.2,
        },
    )))
}

#[get("/shares/<id>")]
pub async fn get_share(
    id: &str,
    pool: &State<PgPool>,
    limiter: &State<std::sync::Arc<ApiRateLimiter>>,
    addr: &ClientRealAddr,
) -> Result<json::Json<SharedResult>, Status> {
    if !limiter.check(&addr.ip) {
        return Err(Status::TooManyRequests);
    }

    let id = id.parse::<Uuid>().map_err(|_| Status::NotFound)?;
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT
             id::text,
             to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),
             to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),
             snapshot::text
         FROM shared_results
         WHERE id = $1 AND expires_at > NOW()",
    )
    .bind(id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|error| {
        warn!("api: failed to load shared result {id}: {error}");
        Status::InternalServerError
    })?
    .ok_or(Status::NotFound)?;

    let snapshot = serde_json::from_str(&row.3).map_err(|error| {
        warn!("api: failed to decode shared result {id}: {error}");
        Status::InternalServerError
    })?;

    Ok(json::Json(SharedResult {
        id: row.0,
        created_at: row.1,
        expires_at: row.2,
        snapshot,
    }))
}

pub async fn cleanup_expired_shares(pool: PgPool) {
    let mut interval = rocket::tokio::time::interval(SHARE_CLEANUP_INTERVAL);
    interval.tick().await;
    loop {
        interval.tick().await;
        if let Err(error) = sqlx::query("DELETE FROM shared_results WHERE expires_at <= NOW()")
            .execute(&pool)
            .await
        {
            warn!("api: failed to clean up expired shared results: {error}");
        }
    }
}
