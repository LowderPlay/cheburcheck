#[macro_use]
extern crate rocket;
mod agency;
mod db;
mod whitelist;

use crate::db::{check_whitelist, save_query};
use log::error;
use querying::resolver::Resolver;
use querying::target::Target;
use querying::{Check, CheckError, CheckVerdict, Checker};
use rocket::fairing::AdHoc;
use rocket::fs::FileServer;
use rocket::http::Status;
use rocket::response::content::RawJavaScript;
use rocket::tokio::sync::RwLock;
use rocket::tokio::time;
use rocket::{fairing, tokio, Build, Request, Rocket, State};
use rocket_cache_response::CacheResponse;
use rocket_client_addr::ClientRealAddr;
use rocket_db_pools::{Connection, Database};
use rocket_dyn_templates::{context, Metadata, Template};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use rocket::serde::json::Json;
use sqlx::types::Uuid;

#[derive(rocket_db_pools::Database)]
#[database("cheburcheck")]
struct Db(sqlx::PgPool);

#[derive(Serialize)]
struct GlobalContext {
    version: &'static str,
}

impl GlobalContext {
    fn new() -> Self {
        GlobalContext {
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

#[get("/")]
async fn index(checker: &State<Arc<RwLock<Checker>>>) -> Template {
    let checker_ref = checker.read().await;
    Template::render(
        "index",
        context! {
            global: GlobalContext::new(),
            domain_count: format_number(checker_ref.total_domains().await),
            v4_count: format_number(checker_ref.total_v4s().await),
            last_update: checker_ref.last_update(),
        },
    )
}

#[get("/kb/<page>")]
fn page(metadata: Metadata, page: &str) -> Option<Template> {
    let page = format!("pages/{}", page);
    if !metadata.contains_template(&page) {
        return None;
    }

    Some(Template::render(
        page,
        context! {
            global: GlobalContext::new(),
        },
    ))
}

#[get("/healthcheck")]
async fn healthcheck(checker: &State<Arc<RwLock<Checker>>>) -> (Status, String) {
    if checker.read().await.last_update().is_some() {
        (Status::Ok, "OK".to_string())
    } else {
        (Status::InternalServerError, "LOADING DATABASES".to_string())
    }
}

#[post("/feedback/<uuid>/<works>")]
async fn feedback(uuid: &str, works: bool, mut db: Connection<Db>, addr: &ClientRealAddr) -> Result<(), Status> {
    sqlx::query!(
        "INSERT INTO human_reports (id, source_ip, works) VALUES ($1, $2, $3)",
        Uuid::try_parse(uuid).map_err(|_| Status::BadRequest)?,
        addr.ip.to_string(),
        works
    ).execute(&mut **db).await.map_err(|_| Status::InternalServerError)?;

    Ok(())
}

#[get("/check?<target>")]
async fn check(
    target: &str,
    checker: &State<Arc<RwLock<Checker>>>,
    addr: &ClientRealAddr,
    mut db: Connection<Db>,
) -> Result<Template, Status> {
    let target = Target::from(target);
    let check = checker.read().await.check(target.clone()).await;
    let id = if let Ok(check) = &check {
        match save_query(&mut db, &target, check, addr, checker.read().await).await {
            Ok(id) => Some(id.to_string()),
            Err(e) => {
                warn!("Failed to save check: {:?}", e);
                None
            }
        }
    } else {
        None
    };

    let whitelist = if let Target::Domain(domain) = &target {
        check_whitelist(domain, &mut db)
            .await
            .map_err(|_| Status::InternalServerError)?
    } else {
        None
    };

    match check {
        Err(CheckError::NotFound) => Ok(Template::render(
            "empty",
            context! {
                global: GlobalContext::new(),
                target: target.to_query(),
                target_type: target.readable_type(),
            },
        )),
        Ok(Check {
            verdict: CheckVerdict::Clear,
            geo,
            ips,
            rkn_subnets,
        }) => Ok(Template::render(
            "result",
            context! {
                id,
                global: GlobalContext::new(),
                found: false,
                target: target.to_query(),
                target_type: target.readable_type(),
                blocked_subnets: rkn_subnets.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>(),
                whitelist,
                ips,
                geo,
            },
        )),
        Ok(Check {
            verdict:
                CheckVerdict::Blocked {
                    rkn_domain,
                    cdn_provider_subnets,
                },
            geo,
            rkn_subnets,
            ips,
        }) => Ok(Template::render(
            "result",
            context! {
                id,
                global: GlobalContext::new(),
                found: true,
                domain: rkn_domain,
                providers: cdn_provider_subnets,
                blocked_subnets: rkn_subnets.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>(),
                target: target.to_query(),
                target_type: target.readable_type(),
                whitelist,
                ips,
                geo,
            },
        )),
        Err(e) => {
            error!("check failed {:?}", e);
            Err(Status::InternalServerError)
        }
    }
}

#[catch(default)]
fn default(status: Status, _req: &Request) -> Template {
    Template::render(
        "error",
        context! {
            global: GlobalContext::new(),
            status: status.code,
            reason: status.reason_lossy(),
        },
    )
}

#[derive(Debug, Serialize)]
struct JsonError {
    code: u16,
    info: String,
}

#[catch(default)]
fn api_error(status: Status, _: &Request) -> Json<JsonError> {
    Json(JsonError { code: status.code, info: status.reason_lossy().to_string() })
}

#[rocket::get("/lucide.js")]
fn lucide() -> CacheResponse<RawJavaScript<&'static [u8]>> {
    CacheResponse::Public {
        responder: RawJavaScript(include_bytes!(concat!(env!("OUT_DIR"), "/lucide.js"))),
        max_age: 604800,
        must_revalidate: false,
    }
}
#[rocket::get("/chart.js")]
fn chartjs() -> CacheResponse<RawJavaScript<&'static [u8]>> {
    CacheResponse::Public {
        responder: RawJavaScript(include_bytes!(concat!(env!("OUT_DIR"), "/chart.js"))),
        max_age: 604800,
        must_revalidate: false,
    }
}
#[rocket::get("/chartjs-plugin-datalabels.js")]
fn chartjs_datalabels() -> CacheResponse<RawJavaScript<&'static [u8]>> {
    CacheResponse::Public {
        responder: RawJavaScript(include_bytes!(concat!(env!("OUT_DIR"), "/chartjs-plugin-datalabels.js"))),
        max_age: 604800,
        must_revalidate: false,
    }
}

fn format_number(number: usize) -> String {
    number
        .to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(" ")
}

async fn run_migrations(rocket: Rocket<Build>) -> fairing::Result {
    match Db::fetch(&rocket) {
        Some(db) => match sqlx::migrate!("./migrations").run(&**db).await {
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
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
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
            log::info!("Updating all DBs");
            checker_clone.read().await.update_all().await;
            log::info!("Updated databases");
        }
    });

    let figment = rocket::Config::figment().merge((
        "databases.cheburcheck.url",
        dotenvy::var("DATABASE_URL").expect("DATABASE_URL must be set"),
    ));

    rocket::custom(figment)
        .manage(Resolver::new().await)
        .manage(checker)
        .attach(Db::init())
        .attach(AdHoc::try_on_ignite("SQLx Migrations", run_migrations))
        .mount("/", routes![index, check, healthcheck, page, feedback])
        .mount("/vendor", routes![lucide, chartjs, chartjs_datalabels])
        .mount("/agency", routes![agency::upload_report])
        .mount("/whitelist", routes![whitelist::histogram, whitelist::export_csv])
        .register("/agency", catchers![api_error])
        .register("/", catchers![default])
        .mount("/", FileServer::from(PathBuf::from("static")))
        .attach(Template::fairing())
}
