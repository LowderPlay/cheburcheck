use crate::agency::Agency;
use crate::Db;
use querying::target::Target;
use querying::{Check, CheckVerdict, Checker};
use rocket::http::Status;
use rocket::outcome::{try_outcome, IntoOutcome};
use rocket::request::{FromRequest, Outcome};
use rocket::tokio::sync::RwLockReadGuard;
use rocket::Request;
use rocket_client_addr::ClientRealAddr;
use rocket_db_pools::Connection;
use serde::Serialize;
use sqlx::types::chrono::NaiveDateTime;
use sqlx::types::Uuid;

pub async fn save_query(
    db: &mut Connection<Db>,
    target: &Target,
    check: &Check,
    addr: &ClientRealAddr,
    checker: RwLockReadGuard<'_, Checker>,
) -> Result<Uuid, sqlx::Error> {
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

    let id = sqlx::query_scalar(
        "INSERT INTO queries (
                     query,
                     source_ip,
                     source_country_code,
                     source_city_geo_name_id,
                     target_country_code,
                     target_asn,
                     target_provider,
                     resolved_ips,
                     cdn_networks,
                     cdn_providers,
                     rkn_domain
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(target.to_query())
    .bind(addr.ip.to_string())
    .bind(
        checker
            .geo_ip(addr.ip)
            .await
            .map(|i| i.country_code)
            .ok()
            .flatten(),
    )
    .bind(check.geo.city_geo_name_id.map(|id| id as i32))
    .bind(check.geo.country_code.clone())
    .bind(check.geo.asn.clone())
    .bind(check.geo.organisation.clone())
    .bind(
        check
            .ips
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>(),
    )
    .bind(cdn_networks)
    .bind(cdn_providers)
    .bind(rkn_domain)
    .fetch_one(&mut ***db)
    .await?;

    Ok(id)
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Agency {
    type Error = Option<rocket_db_pools::Error<sqlx::Error>>;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let mut db = try_outcome!(Connection::<Db>::from_request(request).await);
        let token = request.headers().get_one("Authorization");

        let token = try_outcome!(
            token
                .and_then(|t| t.split_once(" "))
                .map(|(_, tok)| tok.to_string())
                .or_forward(Status::Unauthorized)
        );

        let agency = try_outcome!(
            sqlx::query!("SELECT id, name FROM reporters WHERE token = $1", token)
                .fetch_optional(&mut **db)
                .await
                .map_err(|e| Some(rocket_db_pools::Error::Get(e)))
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
    db: &mut Connection<Db>,
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
    .fetch_optional(&mut ***db)
    .await
    .into()
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WhitelistHistogramBin {
    pub bin_id: Option<i32>,
    pub bin_min_rank: Option<i32>,
    pub bin_max_rank: Option<i32>,
    pub count: Option<i64>,
}

pub async fn collect_histogram(
    db: &mut Connection<Db>,
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
ORDER BY b.bin;", bins, limit / bins, filter
    )
    .fetch_all(&mut ***db)
    .await
    .into()
}
