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
use rocket::http::{ContentType, Status};
use rocket::response::content::RawHtml;
use rocket::serde::json::Json;
use rocket::tokio::sync::RwLock;
use rocket::tokio::time;
use rocket::{Build, Request, Rocket, fairing, tokio};
use serde::Serialize;
use sqlx::postgres::PgPool;
use std::path::PathBuf;
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

#[get("/")]
fn frontend_index() -> Option<RawHtml<&'static str>> {
    frontend::DIST
        .get_file("index.html")
        .and_then(frontend::File::contents_utf8)
        .map(RawHtml)
}

#[get("/<path..>", rank = 20)]
fn frontend_asset(path: PathBuf) -> Option<(ContentType, &'static [u8])> {
    if is_backend_path(&path) {
        return None;
    }

    if let Some(file) = embedded_file(&path) {
        return Some((content_type(&path), file.contents()));
    }

    if path.extension().is_none() {
        let index = frontend::DIST.get_file("index.html")?;
        return Some((ContentType::HTML, index.contents()));
    }

    None
}

fn embedded_file(path: &PathBuf) -> Option<&'static frontend::File<'static>> {
    let path = path.to_string_lossy().replace('\\', "/");
    frontend::DIST.get_file(path)
}

fn is_backend_path(path: &PathBuf) -> bool {
    matches!(
        path.components()
            .next()
            .and_then(|component| component.as_os_str().to_str()),
        Some("api" | "agency" | "whitelist")
    )
}

fn content_type(path: &PathBuf) -> ContentType {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(ContentType::from_extension)
        .unwrap_or(ContentType::Binary)
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
        .mount("/", routes![api::feedback]) // DEPRECATED: backwards compatibility
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
        .mount("/", routes![frontend_index, frontend_asset])
        .register("/", catchers![api_error])
}
