use crate::mqtt::{MqttPublisher, ProbeStatusSnapshot};
use rand::distr::{Alphanumeric, SampleString};
use reports::probe::{ProbeCommand, ProbeCommandResult};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::serde::json::Json;
use rocket::{Request, State};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use std::net::IpAddr;

pub struct Admin;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Admin {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let configured = std::env::var("MQTT_ADMIN_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty());
        let supplied = request
            .headers()
            .get_one("Authorization")
            .and_then(|s| s.strip_prefix("Bearer "));
        match (configured, supplied) {
            (Some(expected), Some(actual))
                if constant_time_eq(expected.as_bytes(), actual.as_bytes()) =>
            {
                Outcome::Success(Admin)
            }
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut difference = a.len() ^ b.len();
    for (left, right) in a.iter().zip(b.iter()) {
        difference |= (left ^ right) as usize;
    }
    difference == 0
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ProbeRow {
    id: i32,
    name: String,
    region: Option<String>,
    asn: Option<String>,
    provider: Option<String>,
    hidden: bool,
    disable_traceroutes: bool,
    cdn_unblocked: bool,
    last_connected_at: Option<DateTime<Utc>>,
    online: bool,
    version: Option<String>,
    bundle_type: Option<String>,
    dpi_hop_v4: Option<i16>,
    dpi_hop_v6: Option<i16>,
    #[sqlx(skip)]
    dpi_hops_v4: Vec<reports::probe::DpiProbeHop>,
    #[sqlx(skip)]
    dpi_hops_v6: Vec<reports::probe::DpiProbeHop>,
}

async fn rows(pool: &PgPool, mqtt: &MqttPublisher) -> Result<Vec<ProbeRow>, Status> {
    let mut rows: Vec<ProbeRow> = sqlx::query_as(
        "SELECT id, name, region, asn, provider, hidden, disable_traceroutes, cdn_unblocked, last_connected_at,
                false AS online, NULL::text AS version, NULL::text AS bundle_type, NULL::smallint AS dpi_hop_v4, NULL::smallint AS dpi_hop_v6
         FROM reporters ORDER BY id"
    ).fetch_all(pool).await.map_err(|error| {
        log::error!("failed to load admin probe list: {error}");
        Status::InternalServerError
    })?;
    let statuses = mqtt.probe_statuses().await;
    for row in &mut rows {
        if let Some(ProbeStatusSnapshot {
            online,
            version,
            bundle_type,
            dpi_hop_v4,
            dpi_hop_v6,
            dpi_hops_v4,
            dpi_hops_v6,
        }) = statuses.get(&row.id.to_string())
        {
            row.online = *online;
            row.version = Some(version.clone());
            row.bundle_type = bundle_type.clone();
            row.dpi_hop_v4 = dpi_hop_v4.map(i16::from);
            row.dpi_hop_v6 = dpi_hop_v6.map(i16::from);
            row.dpi_hops_v4 = dpi_hops_v4.clone();
            row.dpi_hops_v6 = dpi_hops_v6.clone();
        }
    }
    Ok(rows)
}

#[get("/probes")]
pub async fn list(
    _admin: Admin,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<Vec<ProbeRow>>, Status> {
    Ok(Json(rows(pool, mqtt).await?))
}

#[derive(Deserialize)]
pub struct ProbeInput {
    name: String,
    region: Option<String>,
    asn: Option<String>,
    provider: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    disable_traceroutes: bool,
    #[serde(default)]
    cdn_unblocked: bool,
}

fn validate(input: &ProbeInput) -> Result<(), Status> {
    if input.name.trim().is_empty()
        || input.name.len() > 255
        || input.region.as_ref().is_some_and(|s| s.len() > 255)
        || input.asn.as_ref().is_some_and(|s| s.len() > 32)
        || input.provider.as_ref().is_some_and(|s| s.len() > 255)
    {
        return Err(Status::BadRequest);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct CreatedProbe {
    id: i32,
    token: String,
}

#[post("/probes", format = "json", data = "<input>")]
pub async fn create(
    _admin: Admin,
    input: Json<ProbeInput>,
    pool: &State<PgPool>,
) -> Result<Json<CreatedProbe>, Status> {
    validate(&input)?;
    let token = Alphanumeric.sample_string(&mut rand::rng(), 16);
    let id: i32 = sqlx::query_scalar("INSERT INTO reporters (name, token, region, asn, provider, hidden, disable_traceroutes, cdn_unblocked)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(input.name.trim()).bind(&token).bind(&input.region).bind(&input.asn).bind(&input.provider)
        .bind(input.hidden).bind(input.disable_traceroutes).bind(input.cdn_unblocked)
        .fetch_one(&**pool).await.map_err(|_| Status::InternalServerError)?;
    Ok(Json(CreatedProbe { id, token }))
}

#[put("/probes/<id>", format = "json", data = "<input>")]
pub async fn update(
    _admin: Admin,
    id: i32,
    input: Json<ProbeInput>,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<ProbeRow>, Status> {
    validate(&input)?;
    let updated: Option<i32> = sqlx::query_scalar("UPDATE reporters SET name=$2, region=$3, asn=$4, provider=$5, hidden=$6, disable_traceroutes=$7, cdn_unblocked=$8 WHERE id=$1 RETURNING id")
        .bind(id).bind(input.name.trim()).bind(&input.region).bind(&input.asn).bind(&input.provider)
        .bind(input.hidden).bind(input.disable_traceroutes).bind(input.cdn_unblocked)
        .fetch_optional(&**pool).await.map_err(|_| Status::InternalServerError)?;
    if updated.is_none() {
        return Err(Status::NotFound);
    }
    rows(pool, mqtt)
        .await?
        .into_iter()
        .find(|row| row.id == id)
        .map(Json)
        .ok_or(Status::NotFound)
}

#[derive(Serialize)]
pub struct RemovedProbe {
    removed: bool,
}

#[delete("/probes/<id>")]
pub async fn remove(
    _admin: Admin,
    id: i32,
    pool: &State<PgPool>,
) -> Result<Json<RemovedProbe>, Status> {
    let mut tx = pool.begin().await.map_err(|error| {
        log::error!("failed to begin probe removal {id}: {error}");
        Status::InternalServerError
    })?;
    let exists: Option<i32> =
        sqlx::query_scalar("SELECT id FROM reporters WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| {
                log::error!("failed to lock probe {id} for removal: {error}");
                Status::InternalServerError
            })?;
    if exists.is_none() {
        return Err(Status::NotFound);
    }
    sqlx::query("UPDATE reports SET reporter = NULL WHERE reporter = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            log::error!("failed to detach reports from probe {id}: {error}");
            Status::InternalServerError
        })?;
    sqlx::query("DELETE FROM reporters WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            log::error!("failed to delete probe {id}: {error}");
            Status::InternalServerError
        })?;
    tx.commit().await.map_err(|error| {
        log::error!("failed to commit probe removal {id}: {error}");
        Status::InternalServerError
    })?;
    Ok(Json(RemovedProbe { removed: true }))
}

#[post("/probe-config/reload")]
pub async fn reload(
    _admin: Admin,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<reports::probe::ProbeConfig>, Status> {
    mqtt.reload_probe_config()
        .await
        .map(Json)
        .map_err(|_| Status::InternalServerError)
}

#[derive(Serialize)]
pub struct UpdateCheckResponse {
    requested: bool,
}

#[post("/probes/update-check")]
pub async fn update_all_probes(
    _admin: Admin,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<UpdateCheckResponse>, Status> {
    mqtt.request_probe_update(None)
        .await
        .map_err(|_| Status::ServiceUnavailable)?;
    Ok(Json(UpdateCheckResponse { requested: true }))
}

#[post("/probes/<id>/update-check")]
pub async fn update_one_probe(
    _admin: Admin,
    id: i32,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<UpdateCheckResponse>, Status> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reporters WHERE id=$1)")
        .bind(id)
        .fetch_one(&**pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    if !exists {
        return Err(Status::NotFound);
    }
    mqtt.request_probe_update(Some(&id.to_string()))
        .await
        .map_err(|_| Status::ServiceUnavailable)?;
    Ok(Json(UpdateCheckResponse { requested: true }))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandInput {
    ResubscribeTasks,
    RemeasureDpiHop,
    SniTraceroute {
        host: String,
        sni: String,
        max_hops: u8,
    },
    Traceroute {
        target: IpAddr,
        max_hops: u8,
    },
}

#[post("/probes/<id>/commands", format = "json", data = "<input>")]
pub async fn command(
    _admin: Admin,
    id: i32,
    input: Json<CommandInput>,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<ProbeCommandResult>, Status> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reporters WHERE id=$1)")
        .bind(id)
        .fetch_one(&**pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    if !exists {
        return Err(Status::NotFound);
    }
    let command = match input.into_inner() {
        CommandInput::ResubscribeTasks => ProbeCommand::ResubscribeTasks,
        CommandInput::RemeasureDpiHop => ProbeCommand::RemeasureDpiHop,
        CommandInput::SniTraceroute {
            host,
            sni,
            max_hops,
        } => {
            let host = host.trim().to_owned();
            let sni = sni.trim().to_owned();
            if !(1..=64).contains(&max_hops)
                || reports::probe::validate_sni_traceroute(&host, &sni).is_err()
            {
                return Err(Status::BadRequest);
            }
            ProbeCommand::SniTraceroute {
                host,
                sni,
                max_hops,
            }
        }
        CommandInput::Traceroute { target, max_hops } if (1..=64).contains(&max_hops) => {
            ProbeCommand::Traceroute { target, max_hops }
        }
        CommandInput::Traceroute { .. } => return Err(Status::BadRequest),
    };
    mqtt.send_command(&id.to_string(), command)
        .await
        .map(Json)
        .map_err(|error| match error {
            crate::mqtt::PublishError::CommandTimeout => Status::GatewayTimeout,
            _ => Status::ServiceUnavailable,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_probe_fields() {
        let mut input = ProbeInput {
            name: "test".into(),
            region: None,
            asn: None,
            provider: None,
            hidden: false,
            disable_traceroutes: false,
            cdn_unblocked: false,
        };
        assert!(validate(&input).is_ok());
        input.name = " ".into();
        assert!(validate(&input).is_err());
        input.name = "test".into();
        input.asn = Some("x".repeat(33));
        assert!(validate(&input).is_err());
    }

    #[test]
    fn compares_passwords_without_accepting_prefixes() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secre"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
    }
}
