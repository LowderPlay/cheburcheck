#[macro_use]
extern crate rocket;
mod agency;
mod api;
mod database_refresh;
mod db;
mod mqtt;
mod mqtt_auth;
mod probe_installer;
mod probe_updates;
mod whitelist;

use env_logger::Env;
use log::{LevelFilter, error};
use rocket::fairing::AdHoc;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{Build, Request, Rocket, fairing};
use serde::Serialize;
use sqlx::postgres::PgPool;
use std::time::Duration;

#[derive(Debug, Serialize)]
struct JsonError {
    code: u16,
    info: String,
}

#[catch(default)]
fn api_error(status: Status, _: &Request) -> Json<JsonError> {
    Json(JsonError {
        code: status.code,
        info: status.reason_lossy().to_string(),
    })
}

async fn run_migrations(rocket: Rocket<Build>) -> fairing::Result {
    match rocket.state::<PgPool>() {
        Some(db) => match sqlx::migrate!("./migrations").run(db).await {
            Ok(_) => Ok(rocket),
            Err(e) => {
                error!("Failed to run database migrations: {}", e);
                Err(rocket)
            }
        },
        None => Err(rocket),
    }
}

#[launch]
async fn rocket() -> _ {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn"))
        .filter_module("website", LevelFilter::Info)
        .filter_module("querying", LevelFilter::Info)
        .init();

    let checker = database_refresh::start().await;

    let rate_limit_rpm: u32 = std::env::var("API_RATE_LIMIT_RPM")
        .unwrap_or("30".to_string())
        .parse()
        .unwrap_or(30);
    let probe_rate_limit_rpm: u32 = std::env::var("PROBE_RATE_LIMIT_RPM")
        .unwrap_or("5".to_string())
        .parse()
        .unwrap_or(5);
    let probe_update_download_rate_limit_rpm: u32 =
        std::env::var("PROBE_UPDATE_DOWNLOAD_RATE_LIMIT_RPM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
    let api_limiter = std::sync::Arc::new(api::build_rate_limiter(rate_limit_rpm));
    let probe_limiter = std::sync::Arc::new(api::build_probe_rate_limiter(probe_rate_limit_rpm));
    let probe_update_download_limiter = std::sync::Arc::new(
        api::build_probe_update_download_rate_limiter(probe_update_download_rate_limit_rpm),
    );
    let mqtt_publisher = mqtt::MqttPublisher::start_from_env();
    let probe_update_proxy = probe_updates::ProbeUpdateProxy::from_env()
        .expect("failed to configure probe update proxy");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(
            std::env::var("DATABASE_MAX_CONNECTIONS")
                .unwrap_or("100".to_string())
                .parse()
                .unwrap(),
        )
        .min_connections(
            std::env::var("DATABASE_MIN_CONNECTIONS")
                .unwrap_or("10".to_string())
                .parse()
                .unwrap(),
        )
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(60))
        .connect(&dotenvy::var("DATABASE_URL").expect("DATABASE_URL must be set"))
        .await
        .expect("Failed to create database pool");

    rocket::build()
        .manage(checker)
        .manage(pool)
        .manage(api_limiter)
        .manage(probe_limiter)
        .manage(probe_update_download_limiter)
        .manage(mqtt_publisher)
        .manage(probe_update_proxy)
        .attach(AdHoc::try_on_ignite("SQLx Migrations", run_migrations))
        .mount(
            "/api/v1",
            routes![
                api::check,
                api::probe_query,
                api::healthcheck,
                api::feedback,
                api::get_system_status,
                api::node_stats,
                whitelist::histogram
            ],
        )
        .mount("/agency", routes![agency::upload_report])
        .mount("/mqtt", routes![mqtt_auth::auth, mqtt_auth::acl])
        .mount(
            "/api/v1/probe-updates",
            routes![probe_updates::latest_release, probe_updates::download_asset],
        )
        .mount("/", routes![probe_installer::download])
        .mount("/whitelist", routes![whitelist::export_csv])
        .register("/", catchers![api_error])
}
