use crate::mqtt::{MqttPublisher, ProbeStatusSnapshot};
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::serde::json::Json;
use rocket::{Request, State};
use serde::Serialize;
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(sqlx::FromRow)]
struct ProbeMetadata {
    id: i32,
    name: String,
    region: Option<String>,
    provider: Option<String>,
    asn: Option<String>,
    last_connection_ip: Option<String>,
    last_connected_at: Option<DateTime<Utc>>,
}

pub struct NodeStatsKey;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for NodeStatsKey {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let expected_key = std::env::var("NODE_STATS_KEY")
            .ok()
            .filter(|key| !key.is_empty());
        let provided_key = request
            .headers()
            .get_one("Authorization")
            .and_then(|header| header.strip_prefix("Bearer "));

        if valid_key(provided_key, expected_key.as_deref()) {
            Outcome::Success(Self)
        } else {
            Outcome::Error((Status::Unauthorized, ()))
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NodeStatsResponse {
    nodes: Vec<NodeStatus>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct NodeStatus {
    probe_id: i32,
    name: String,
    region: Option<String>,
    provider: Option<String>,
    asn: Option<String>,
    connection_ip: Option<String>,
    connected_at: Option<DateTime<Utc>>,
    online: bool,
    version: Option<String>,
    dpi_hop_v4: Option<u8>,
    dpi_hop_v6: Option<u8>,
}

#[get("/nodes")]
pub async fn node_stats(
    _key: NodeStatsKey,
    pool: &State<PgPool>,
    mqtt: &State<MqttPublisher>,
) -> Result<Json<NodeStatsResponse>, Status> {
    let probes = sqlx::query_as::<_, ProbeMetadata>(
        "SELECT id, name, region, provider, asn, last_connection_ip, last_connected_at
         FROM reporters
         ORDER BY id",
    )
    .fetch_all(&**pool)
    .await
    .map_err(|error| {
        log::error!("failed to load probe metadata for node stats: {error}");
        Status::InternalServerError
    })?;

    let statuses = mqtt.probe_statuses().await;
    Ok(Json(NodeStatsResponse {
        nodes: build_node_statuses(probes, &statuses),
    }))
}

fn valid_key(provided: Option<&str>, expected: Option<&str>) -> bool {
    provided
        .zip(expected)
        .is_some_and(|(provided, expected)| provided == expected && !provided.is_empty())
}

fn build_node_statuses(
    probes: Vec<ProbeMetadata>,
    statuses: &HashMap<String, ProbeStatusSnapshot>,
) -> Vec<NodeStatus> {
    probes
        .into_iter()
        .map(|probe| {
            let status = statuses.get(&probe.id.to_string());
            NodeStatus {
                probe_id: probe.id,
                name: probe.name,
                region: probe.region,
                provider: probe.provider,
                asn: probe.asn,
                connection_ip: probe.last_connection_ip,
                connected_at: probe.last_connected_at,
                online: status.is_some_and(|status| status.online),
                version: status.map(|status| status.version.clone()),
                dpi_hop_v4: status.and_then(|status| status.dpi_hop_v4),
                dpi_hop_v6: status.and_then(|status| status.dpi_hop_v6),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_a_configured_matching_key() {
        assert!(valid_key(Some("secret"), Some("secret")));
        assert!(!valid_key(Some("wrong"), Some("secret")));
        assert!(!valid_key(None, Some("secret")));
        assert!(!valid_key(Some("secret"), None));
        assert!(!valid_key(Some(""), Some("")));
    }

    #[test]
    fn builds_online_and_offline_json_nodes() {
        let probes = vec![
            ProbeMetadata {
                id: 1,
                name: "Home router".to_string(),
                region: Some("Ural".to_string()),
                provider: Some("ExampleNet".to_string()),
                asn: Some("AS64500".to_string()),
                last_connection_ip: Some("203.0.113.10".to_string()),
                last_connected_at: Some("2026-08-24T12:34:56Z".parse().unwrap()),
            },
            ProbeMetadata {
                id: 2,
                name: "Offline".to_string(),
                region: None,
                provider: None,
                asn: None,
                last_connection_ip: Some("2001:db8::10".to_string()),
                last_connected_at: Some("2026-08-23T12:34:56Z".parse().unwrap()),
            },
        ];
        let statuses = HashMap::from([(
            "1".to_string(),
            ProbeStatusSnapshot {
                online: true,
                version: "1.2.3".to_string(),
                dpi_hop_v4: Some(5),
                dpi_hop_v6: None,
            },
        )]);

        let nodes = build_node_statuses(probes, &statuses);

        assert_eq!(
            nodes,
            vec![
                NodeStatus {
                    probe_id: 1,
                    name: "Home router".to_string(),
                    region: Some("Ural".to_string()),
                    provider: Some("ExampleNet".to_string()),
                    asn: Some("AS64500".to_string()),
                    connection_ip: Some("203.0.113.10".to_string()),
                    connected_at: Some("2026-08-24T12:34:56Z".parse().unwrap()),
                    online: true,
                    version: Some("1.2.3".to_string()),
                    dpi_hop_v4: Some(5),
                    dpi_hop_v6: None,
                },
                NodeStatus {
                    probe_id: 2,
                    name: "Offline".to_string(),
                    region: None,
                    provider: None,
                    asn: None,
                    connection_ip: Some("2001:db8::10".to_string()),
                    connected_at: Some("2026-08-23T12:34:56Z".parse().unwrap()),
                    online: false,
                    version: None,
                    dpi_hop_v4: None,
                    dpi_hop_v6: None,
                },
            ]
        );
    }
}
