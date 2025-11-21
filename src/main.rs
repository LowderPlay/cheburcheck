mod target;
mod resolver;
mod geoip;
mod lists;
mod updater;

#[macro_use] extern crate rocket;
extern crate maxminddb;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use rocket::fs::FileServer;
use rocket::http::Status;
use rocket::{tokio, Request, State};
use rocket::response::content::RawJavaScript;
use rocket_dyn_templates::{context, Metadata, Template};
use serde::Serialize;
use log::error;
use rocket::tokio::sync::{watch, RwLock};
use rocket::tokio::time;
use rocket_cache_response::CacheResponse;
use crate::geoip::{GeoIp, IpInfo};
use crate::lists::{CdnList, NetworkRecord, RuBlacklist};
use crate::resolver::Resolver;
use crate::target::Target;
use crate::updater::update_all;

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
fn index(info: &State<watch::Receiver<UpdateInfo>>) -> Template {
    Template::render("index", context! {
        global: GlobalContext::new(),
        info: &*info.borrow(),
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
fn healthcheck(info: &State<watch::Receiver<UpdateInfo>>) -> (Status, String) {
    if info.borrow().last_update.is_some() {
        (Status::Ok, "OK".to_string())
    } else {
        (Status::InternalServerError, "LOADING DATABASES".to_string())
    }
}

#[get("/check?<target>")]
async fn check(target: &str, resolver: &State<Resolver>,
               geo_ip: &State<Arc<RwLock<GeoIp>>>,
               cdn: &State<Arc<RwLock<CdnList>>>,
               ru_blacklist: &State<Arc<RwLock<RuBlacklist>>>) -> Result<Template, Status> {
    let target = Target::from(target);
    let ips = match target.resolve(resolver).await {
        Ok(ips) => ips,
        Err(e) if e.kind().is_no_records_found() => {
            return Ok(Template::render("empty", context! {
                global: GlobalContext::new(),
                target: target.to_query(),
                target_type: target.readable_type(),
            }));
        }
        Err(e) => {
            error!("{}", e);
            return Err(Status::BadRequest);
        },
    };
    let geo_ip = geo_ip.read().await;
    let geo = match ips.get(0).map(|ip| geo_ip.lookup(ip.clone())) {
        None => IpInfo::default(),
        Some(Ok(ip)) => ip,
        Some(Err(e)) => {
            error!("{}", e);
            return Err(Status::BadRequest);
        },
    };
    let mut providers: HashMap<String, HashSet<NetworkRecord>> = HashMap::new();

    let cdn = cdn.read().await;
    ips.iter()
        .filter_map(|ip| cdn.contains(ip))
        .map(|ip| (match &ip.region {
            None => ip.provider.clone(),
            Some(region) => format!("{} ({})", ip.provider, region),
        }, ip.clone()))
        .for_each(|(k, v)| {
            providers.entry(k).or_default().insert(v);
        });

    let ru_blacklist = ru_blacklist.read().await;
    let domain = match &target {
        Target::Domain(domain) => ru_blacklist.contains_domain(domain),
        _ => None
    };

    let blocked_subnets: HashSet<String> = ips.iter()
        .filter_map(|ip| ru_blacklist.contains_ip(ip))
        .map(|ip| ip.to_string()).collect();

    Ok(Template::render("result", context! {
        global: GlobalContext::new(),
        found: providers.len() > 0 || domain.is_some() || blocked_subnets.len() > 0,
        cdn_found: providers.len() > 0,
        domain_found: domain.is_some(),
        subnet_found: blocked_subnets.len() > 0,
        providers,
        domain,
        blocked_subnets,
        target: target.to_query(),
        target_type: target.readable_type(),
        ips,
        geo,
    }))
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

#[derive(Serialize, Default)]
pub struct UpdateInfo {
    last_update: Option<DateTime<Utc>>,
    domain_count: String,
    v4_count: String,
}

fn format_number(number: u64) -> String {
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

    let (tx, rx) = watch::channel(UpdateInfo::default());
    let cdn_list = Arc::new(RwLock::new(CdnList::new()));
    let rkn_list = Arc::new(RwLock::new(RuBlacklist::new()));
    let geo_ip = Arc::new(RwLock::new(GeoIp::new()));

    let geo_ip_clone = geo_ip.clone();
    let rkn_list_clone = rkn_list.clone();
    let cdn_list_clone = cdn_list.clone();
    tokio::spawn(async move {
        info!("Refreshing DB every {:?}", interval.period());
        loop {
            interval.tick().await;
            update_all(geo_ip_clone.clone(), rkn_list_clone.clone(), cdn_list_clone.clone()).await;
            let domain_count = rkn_list_clone.read().await.domain_count;
            let v4_count = rkn_list_clone.read().await.v4_count() + cdn_list_clone.read().await.v4_count();
            tx.send(UpdateInfo {
                last_update: Some(Utc::now()),
                domain_count: format_number(domain_count as u64),
                v4_count: format_number(v4_count as u64),
            }).unwrap();
        }
    });

    rocket::build()
        .manage(Resolver::new().await)
        .manage(cdn_list)
        .manage(rkn_list)
        .manage(geo_ip)
        .manage(rx)
        .mount("/", routes![index, lucide, check, healthcheck, page])
        .register("/", catchers![default])
        .mount("/", FileServer::from(PathBuf::from("static")))
        .attach(Template::fairing())
}
