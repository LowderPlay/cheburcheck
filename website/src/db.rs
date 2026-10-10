use crate::agency::Agency;
use crate::analytics::Analytics;
use klickhouse::QueryBuilder;
use querying::target::Target;
use querying::{Check, CheckVerdict, Checker};
use rocket::http::Status;
use rocket::outcome::{IntoOutcome, try_outcome};
use rocket::request::{FromRequest, Outcome};
use rocket::tokio::sync::RwLockReadGuard;
use rocket::{Request, State};
use rocket_client_addr::ClientRealAddr;
use serde::Serialize;
use sqlx::types::Uuid;
use sqlx::types::chrono::NaiveDateTime;

pub async fn save_query(
    analytics: &Analytics,
    target: &Target,
    check: &Check,
    addr: &ClientRealAddr,
    checker: RwLockReadGuard<'_, Checker>,
) -> Result<Uuid, klickhouse::KlickhouseError> {
    let (cdn_networks, cdn_providers, rkn_domain): (Vec<_>, Vec<_>, Option<_>) =
        if let CheckVerdict::Blocked {
            cdn_provider_subnets,
            rkn_domain,
            ..
        } = &check.verdict
        {
            (
                cdn_provider_subnets
                    .values()
                    .flatten()
                    .map(|n| n.cidr.to_string())
                    .collect(),
                cdn_provider_subnets.keys().map(|p| p.to_string()).collect(),
                rkn_domain.clone(),
            )
        } else {
            (vec![], vec![], None)
        };

    let (resolved_ips, cdn_networks) = match target {
        Target::Asn(_) => (vec![], vec![]),
        _ => (
            check
                .ips
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<String>>(),
            cdn_networks,
        ),
    };

    let id = Uuid::new_v4();
    let source_country_code = checker
        .geo_ip(addr.ip)
        .await
        .ok()
        .and_then(|info| info.country_code);
    drop(checker);
    analytics
        .execute(
            QueryBuilder::new(crate::analytics::INSERT_QUERY_SQL)
                .arg(id)
                .arg(target.to_query())
                .arg(addr.ip.to_string())
                .arg(source_country_code)
                .arg(check.geo.city_geo_name_id.map(|id| id as i32))
                .arg(check.geo.country_code.clone())
                .arg(check.geo.asn.clone())
                .arg(check.geo.organisation.clone())
                .arg(resolved_ips)
                .arg(cdn_networks)
                .arg(cdn_providers)
                .arg(rkn_domain),
        )
        .await?;

    Ok(id)
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Agency {
    type Error = Option<sqlx::Error>;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let pool = request.guard::<&State<sqlx::PgPool>>().await.unwrap();
        let mut db = try_outcome!(
            pool.acquire()
                .await
                .map_err(Some)
                .or_forward(Status::InternalServerError)
        );
        let token = request.headers().get_one("Authorization");

        let token = try_outcome!(
            token
                .and_then(|t| t.split_once(" "))
                .map(|(_, tok)| tok.to_string())
                .or_forward(Status::Unauthorized)
        );

        let agency = try_outcome!(
            sqlx::query!("SELECT id, name FROM reporters WHERE token = $1", token)
                .fetch_optional(&mut *db)
                .await
                .map_err(Some)
                .or_forward(Status::InternalServerError)
        );
        agency
            .map(|r| Agency {
                id: r.id,
                name: r.name,
            })
            .or_forward(Status::Unauthorized)
    }
}

#[derive(Serialize, Debug, sqlx::FromRow)]
pub struct WhitelistedEntry {
    domain: Option<String>,
    rank: Option<i32>,
    last_ok: Option<NaiveDateTime>,
}

pub async fn check_whitelist(
    domain: &str,
    db: &mut sqlx::PgConnection,
) -> Result<Option<WhitelistedEntry>, sqlx::Error> {
    if domain.chars().filter(|c| *c == '.').count() > 4 {
        return Ok(None);
    }
    sqlx::query_as!(
        WhitelistedEntry,
        "SELECT *
        FROM whitelist
        WHERE $1 = domain
           OR $1 LIKE CONCAT('%.', domain)
        ORDER BY LENGTH(domain) DESC
        LIMIT 1",
        domain
    )
    .fetch_optional(db)
    .await
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WhitelistHistogramBin {
    pub bin_id: Option<i32>,
    pub bin_min_rank: Option<i32>,
    pub bin_max_rank: Option<i32>,
    pub count: Option<i64>,
}

pub async fn collect_histogram(
    db: &mut sqlx::PgConnection,
    bins: i32,
    limit: i32,
    filter: bool,
) -> Result<Vec<WhitelistHistogramBin>, sqlx::Error> {
    sqlx::query_as!(
        WhitelistHistogramBin,
        "WITH bins AS (
  SELECT generate_series(0, $1 - 1) AS bin
)
SELECT
  b.bin as bin_id,
  b.bin * $2 + 1    AS bin_min_rank,
  (b.bin + 1) * $2 AS bin_max_rank,
  COUNT(case when not $3 or w.domain not like '%.co.uk' then 1 end)     AS count
FROM bins b
LEFT JOIN whitelist w
  ON FLOOR(w.rank / $2) = b.bin
GROUP BY b.bin
ORDER BY b.bin;",
        bins,
        limit / bins,
        filter
    )
    .fetch_all(db)
    .await
}
