#[macro_use]
extern crate rocket;
mod agency;
mod api;
mod db;
mod whitelist;

use env_logger::Env;
use log::{LevelFilter, error, info};
use querying::Checker;
use rocket::fairing::AdHoc;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::tokio::sync::RwLock;
use rocket::tokio::time;
use rocket::{Build, Request, Rocket, fairing, tokio};
use serde::Serialize;
use sqlx::postgres::PgPool;
use std::sync::Arc;
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

    let mut interval = time::interval(Duration::from_secs(
        std::env::var("DATABASE_INTERVAL_SECONDS")
            .unwrap_or("21600".to_string())
            .parse()
            .unwrap(),
    ));

    let checker = Arc::new(RwLock::new(Checker::new().await));

    let checker_clone = checker.clone();
    tokio::spawn(async move {
        info!("Refreshing DB every {:?}", interval.period());
        loop {
            interval.tick().await;
            info!("Updating all DBs");
            match Checker::download_all().await {
                Ok(bases) => {
                    info!("Downloaded, updating...");
                    checker_clone.read().await.update_all(bases).await;
                    info!("Updated databases");
                }
                Err(_) => log::error!("Failed to download all DBs"),
            }
        }
    });

    let rate_limit_rpm: u32 = std::env::var("API_RATE_LIMIT_RPM")
        .unwrap_or("30".to_string())
        .parse()
        .unwrap_or(30);
    let api_limiter = std::sync::Arc::new(api::build_rate_limiter(rate_limit_rpm));

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
        .attach(AdHoc::try_on_ignite("SQLx Migrations", run_migrations))
        .mount(
            "/api/v1",
            routes![
                api::check,
                api::healthcheck,
                api::feedback,
                api::get_system_status,
                whitelist::histogram
            ],
        )
        .mount("/agency", routes![agency::upload_report])
        .mount("/whitelist", routes![whitelist::export_csv])
        .register("/", catchers![api_error])
}
