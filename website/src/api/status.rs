use querying::Checker;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::tokio::sync::RwLock;
use serde::Serialize;
use sqlx::types::chrono::{DateTime, Utc};
use std::sync::Arc;

#[get("/healthcheck")]
pub async fn healthcheck(checker: &State<Arc<RwLock<Checker>>>) -> (Status, String) {
    if checker.read().await.last_update().is_some() {
        (Status::Ok, "OK".to_string())
    } else {
        (Status::InternalServerError, "LOADING DATABASES".to_string())
    }
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
