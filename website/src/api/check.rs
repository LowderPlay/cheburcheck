use super::rate_limit::ApiRateLimiter;
use crate::analytics::{Analytics, ComplaintDay};
use crate::check_context::CheckContext;
use crate::db::{WhitelistedEntry, check_whitelist, save_query};
use log::warn;
use querying::asn::AsnInfo;
use querying::geoip::IpInfo;
use querying::lists::NetworkRecord;
use querying::target::Target;
use querying::{Check, CheckError, CheckVerdict, Checker};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::serde::json::{Json, Value, serde_json};
use rocket::tokio::sync::RwLock;
use rocket::{Request, State};
use rocket_client_addr::ClientRealAddr;
use serde::Serialize;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

pub struct UserAgent<'r>(Option<&'r str>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for UserAgent<'r> {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(Self(request.headers().get_one("User-Agent")))
    }
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
    pub reverse_lookup: Vec<String>,
    pub complaints: Vec<ComplaintDay>,
}

#[get("/check?<target>")]
pub async fn check(
    target: &str,
    checker: &State<Arc<RwLock<Checker>>>,
    addr: &ClientRealAddr,
    user_agent: UserAgent<'_>,
    pool: &State<PgPool>,
    analytics: &State<Analytics>,
    contexts: &State<CheckContext>,
    limiter: &State<Arc<ApiRateLimiter>>,
) -> Result<Json<ApiCheckResponse>, Status> {
    if !limiter.check(&addr.ip) {
        return Err(Status::TooManyRequests);
    }

    let target = Target::from(target.trim());
    let check = checker.read().await.check(target.clone()).await;

    let id = if let Ok(check) = &check {
        match save_query(
            analytics,
            &target,
            check,
            addr,
            user_agent.0,
            checker.read().await,
        )
        .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                warn!("api: failed to save check: {:?}", e);
                None
            }
        }
    } else {
        None
    };

    let id = match (&id, &check) {
        (Some(id), Ok(check)) => Some(
            contexts
                .issue(*id, target.to_query(), check.ips.first().copied())
                .map_err(|error| {
                    warn!("api: failed to sign check context: {error}");
                    Status::InternalServerError
                })?,
        ),
        _ => None,
    };

    let whitelist: Option<WhitelistedEntry> = if let Target::Domain(domain) = &target {
        let mut db = pool
            .acquire()
            .await
            .map_err(|_| Status::InternalServerError)?;
        check_whitelist(domain, &mut db)
            .await
            .map_err(|_| Status::InternalServerError)?
    } else {
        None
    };

    let complaints = match &check {
        Ok(_) => match analytics.complaints(&target.to_query()).await {
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

pub async fn build_shared_check_result(
    query: &str,
    checker: &State<Arc<RwLock<Checker>>>,
    pool: &PgPool,
    analytics: &Analytics,
) -> Result<Value, Status> {
    let target = Target::from(query.trim());
    let check = checker.read().await.check(target.clone()).await;
    let check = match check {
        Ok(check) => check,
        Err(CheckError::NotFound) => return Err(Status::NotFound),
        Err(error) => {
            warn!("api: failed to rebuild check for sharing: {error:?}");
            return Err(Status::InternalServerError);
        }
    };

    let whitelist = if let Target::Domain(domain) = &target {
        let mut db = pool.acquire().await.map_err(|_| Status::InternalServerError)?;
        check_whitelist(domain, &mut db)
            .await
            .map_err(|_| Status::InternalServerError)?
    } else {
        None
    };
    let complaints = match analytics.complaints(&target.to_query()).await {
        Ok(days) => days,
        Err(error) => {
            warn!("api: failed to load complaints for shared check: {error}");
            Vec::new()
        }
    };
    let response = build_response(None, &target, check, whitelist, complaints);
    shared_result_value(&response).map_err(|error| {
        warn!("api: failed to serialize rebuilt check for sharing: {error}");
        Status::InternalServerError
    })
}

fn shared_result_value(response: &ApiCheckResponse) -> Result<Value, serde_json::Error> {
    let providers = response
        .cdn_providers
        .iter()
        .map(|(name, networks)| serde_json::json!({ "name": name, "networks": networks }))
        .collect::<Vec<_>>();
    let whitelist = response
        .whitelist
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?
        .map(|entry| {
            serde_json::json!({ "lastOk": entry.get("last_ok").cloned().unwrap_or(Value::Null) })
        });
    let asn_info = response.asn_info.as_ref().map(|info| {
        serde_json::json!({
            "prefixes": info.prefixes,
            "blockedPrefixes": info.blocked_prefixes,
        })
    });
    let result = serde_json::json!({
        "target": response.target,
        "targetType": response.target_type,
        "found": response.blocked,
        "blocked": response.blocked,
        "domain": response.rkn_domain,
        "ips": response.ips,
        "reverseLookup": response.reverse_lookup,
        "subnetSize": response.subnet_size,
        "geo": response.geo,
        "providers": providers,
        "blockedSubnets": response.blocked_subnets,
        "asnInfo": asn_info,
        "whitelist": whitelist,
        "complaints": response.complaints,
    });
    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::Header;
    use rocket::local::asynchronous::Client;

    #[get("/")]
    fn header(user_agent: UserAgent<'_>) -> Json<Option<String>> {
        Json(user_agent.0.map(str::to_owned))
    }

    #[rocket::async_test]
    async fn captures_optional_user_agent_header() {
        let client = Client::tracked(rocket::build().mount("/", routes![header]))
            .await
            .unwrap();
        let agent = "Mozilla/5.0 'quoted' \\ agent $1";
        let response = client
            .get("/")
            .header(Header::new("User-Agent", agent))
            .dispatch()
            .await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(
            response
                .into_json::<Option<String>>()
                .await
                .unwrap()
                .as_deref(),
            Some(agent)
        );
        let response = client.get("/").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(response.into_json::<Option<String>>().await, Some(None));
    }
}
