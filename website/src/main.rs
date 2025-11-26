#[macro_use] extern crate rocket;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use rocket::fs::FileServer;
use rocket::http::Status;
use rocket::{tokio, Request, State};
use rocket::response::content::RawJavaScript;
use rocket_dyn_templates::{context, Metadata, Template};
use serde::Serialize;
use log::error;
use rocket::tokio::sync::RwLock;
use rocket::tokio::time;
use rocket_cache_response::CacheResponse;
use querying::{Check, CheckError, CheckVerdict, Checker};
use querying::resolver::Resolver;
use querying::target::Target;

#[derive(Serialize)]
struct GlobalContext {
    version: &'static str,
}

impl GlobalContext {
    fn new() -> Self {
        GlobalContext { version: env!("CARGO_PKG_VERSION") }
    }
}

#[get("/")]
async fn index(checker: &State<Arc<RwLock<Checker>>>) -> Template {
    let checker_ref = checker.read().await;
    Template::render("index", context! {
        global: GlobalContext::new(),
        domain_count: format_number(checker_ref.total_domains().await),
        v4_count: format_number(checker_ref.total_v4s().await),
        last_update: checker_ref.last_update(),
    })
}

#[get("/kb/<page>")]
fn page(metadata: Metadata, page: &str) -> Option<Template> {
    let page = format!("pages/{}", page);
    if !metadata.contains_template(&page) {
        return None;
    }

    Some(Template::render(page, context! {
        global: GlobalContext::new(),
    }))
}

#[get("/healthcheck")]
async fn healthcheck(checker: &State<Arc<RwLock<Checker>>>) -> (Status, String) {
    if checker.read().await.last_update().is_some() {
        (Status::Ok, "OK".to_string())
    } else {
        (Status::InternalServerError, "LOADING DATABASES".to_string())
    }
}

#[get("/check?<target>")]
async fn check(target: &str, checker: &State<Arc<RwLock<Checker>>>) -> Result<Template, Status> {
    let target = Target::from(target);
    match checker.read().await.check(target.clone()).await {
        Err(CheckError::NotFound) =>
            Ok(Template::render("empty", context! {
                    global: GlobalContext::new(),
                    target: target.to_query(),
                    target_type: target.readable_type(),
                })),
        Ok(Check { verdict: CheckVerdict::Clear, geo, ips }) =>
            Ok(Template::render("result", context! {
                global: GlobalContext::new(),
                found: false,
                target: target.to_query(),
                target_type: target.readable_type(),
                ips,
                geo,
            })),
        Ok(Check {
               verdict: CheckVerdict::Blocked {
                   rkn_domain,
                   rkn_subnets,
                   cdn_provider_subnets
               }, geo, ips }) =>
            Ok(Template::render("result", context! {
                global: GlobalContext::new(),
                found: true,
                domain: rkn_domain,
                providers: cdn_provider_subnets,
                blocked_subnets: rkn_subnets.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>(),
                target: target.to_query(),
                target_type: target.readable_type(),
                ips,
                geo,
            })),
        Err(e) => {
            error!("check failed {:?}", e);
            Err(Status::InternalServerError)
        }
    }
}

#[catch(default)]
fn default(status: Status, _req: &Request) -> Template {
    Template::render("error", context! {
        global: GlobalContext::new(),
        status: status.code,
        reason: status.reason_lossy(),
    })
}

#[rocket::get("/vendor/lucide.js")]
fn lucide() -> CacheResponse<RawJavaScript<&'static [u8]>> {
    CacheResponse::Public {
        responder: RawJavaScript(include_bytes!(concat!(env!("OUT_DIR"), "/lucide.js"))),
        max_age: 604800,
        must_revalidate: false,
    }
}

fn format_number(number: usize) -> String {
    number.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(" ")
}

#[launch]
async fn rocket() -> _ {
    env_logger::builder().filter_level(log::LevelFilter::Info).init();

    let mut interval = time::interval(Duration::from_secs(std::env::var("DATABASE_INTERVAL_SECONDS")
        .unwrap_or("21600".to_string()).parse().unwrap()));

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

    rocket::build()
        .manage(Resolver::new().await)
        .manage(checker)
        .mount("/", routes![index, lucide, check, healthcheck, page])
        .register("/", catchers![default])
        .mount("/", FileServer::from(PathBuf::from("static")))
        .attach(Template::fairing())
}
