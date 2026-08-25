use log::{info, warn};
use reports::probe::HostType;
use reports::probe::{
    DpiProbeConfig, Host, ProbeConfig, ProbeResult, ProbeResultEvent, ProbeStatus, ProbeTask,
};
use rocket::serde::json::serde_json;
use rumqttc::{AsyncClient, Event as MqttEvent, Incoming, MqttOptions, QoS};
use serde::Deserialize;
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

const MQTT_MAX_PACKET_SIZE: usize = 1024 * 1024;

const DEFAULT_PROBE_HOSTS: &str = include_str!("../probe-hosts.toml");

#[derive(Debug)]
pub enum PublishError {
    NotConfigured,
    Config(std::io::Error),
    ConfigParse(toml::de::Error),
    Serialize(serde_json::Error),
    Subscribe(rumqttc::ClientError),
    Publish(rumqttc::ClientError),
}

impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishError::NotConfigured => write!(formatter, "MQTT publisher is not configured"),
            PublishError::Config(error) => {
                write!(formatter, "failed to read probe config: {error}")
            }
            PublishError::ConfigParse(error) => {
                write!(formatter, "failed to parse probe config: {error}")
            }
            PublishError::Serialize(error) => {
                write!(formatter, "failed to serialize task: {error}")
            }
            PublishError::Subscribe(error) => {
                write!(formatter, "failed to subscribe to results: {error}")
            }
            PublishError::Publish(error) => write!(formatter, "failed to publish task: {error}"),
        }
    }
}

#[derive(Clone)]
pub struct MqttPublisher {
    client: Option<AsyncClient>,
    sessions: Arc<rocket::tokio::sync::RwLock<HashMap<String, ProbeResultSender>>>,
    probe_statuses: ProbeStatuses,
    probe_config: Arc<ProbeConfig>,
    task_timeout_ms: u64,
}

type ProbeResultSender = rocket::tokio::sync::broadcast::Sender<ProbeResultEvent>;
pub type ProbeResultReceiver = rocket::tokio::sync::broadcast::Receiver<ProbeResultEvent>;
type ProbeStatuses = Arc<rocket::tokio::sync::RwLock<HashMap<String, ProbeStatusSnapshot>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeStatusSnapshot {
    pub online: bool,
    pub version: String,
    pub dpi_hop_v4: Option<u8>,
    pub dpi_hop_v6: Option<u8>,
}

#[derive(Deserialize)]
struct ProbeHostsFile {
    timeout_sec: u32,
    min_data: u32,
    #[serde(default = "reports::probe::default_dns_samples_per_protocol")]
    dns_samples_per_protocol: u8,
    #[serde(default = "reports::probe::default_dns_spoofing_provider_threshold")]
    dns_spoofing_provider_threshold: u8,
    #[serde(default)]
    dpi_probe: Option<DpiProbeConfig>,
    hosts: Vec<ProbeHostEntry>,
}

#[derive(Deserialize)]
struct ProbeHostEntry {
    id: String,
    host: String,
    host_type: HostType,
    file_path: String,
    timeout_sec: Option<u32>,
    min_data: Option<u32>,
}

impl MqttPublisher {
    pub fn start_from_env() -> Self {
        let sessions = Arc::new(rocket::tokio::sync::RwLock::new(HashMap::new()));
        let probe_statuses = Arc::new(rocket::tokio::sync::RwLock::new(HashMap::new()));
        let task_timeout_ms = task_timeout_ms_from_env();
        let probe_config = Arc::new(load_probe_config(task_timeout_ms).unwrap_or_else(|error| {
            warn!("failed to load probe config: {error}");
            ProbeConfig {
                version: env!("CARGO_PKG_VERSION").to_string(),
                task_timeout_ms,
                published_at: Utc::now().to_rfc3339(),
                hosts: Vec::new(),
                traceroute_enabled: false,
                dns_samples_per_protocol: reports::probe::default_dns_samples_per_protocol(),
                dns_spoofing_provider_threshold:
                    reports::probe::default_dns_spoofing_provider_threshold(),
                dpi_probe: None,
            }
        }));
        let admin_token = match std::env::var("MQTT_ADMIN_TOKEN") {
            Ok(token) if !token.is_empty() => token,
            _ => {
                warn!("mqtt publisher disabled: MQTT_ADMIN_TOKEN is not set");
                return Self {
                    client: None,
                    sessions,
                    probe_statuses,
                    probe_config,
                    task_timeout_ms: task_timeout_ms_from_env(),
                };
            }
        };

        let host = std::env::var("MQTT_HOST").unwrap_or_else(|_| "rmqtt".to_string());
        let port = std::env::var("MQTT_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
            .unwrap_or(11883);
        let client_id =
            std::env::var("MQTT_CLIENT_ID").unwrap_or_else(|_| "website-api".to_string());

        let mut options = MqttOptions::new(client_id, host.clone(), port);
        options.set_credentials("admin", admin_token);
        options.set_keep_alive(Duration::from_secs(10));
        options.set_max_packet_size(MQTT_MAX_PACKET_SIZE, MQTT_MAX_PACKET_SIZE);

        let (client, mut eventloop) = AsyncClient::new(options, 100);
        let event_sessions = sessions.clone();
        let event_probe_statuses = probe_statuses.clone();
        let config_client = client.clone();
        let event_probe_config = probe_config.clone();
        rocket::tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(MqttEvent::Incoming(Incoming::ConnAck(_))) => {
                        if let Err(error) =
                            publish_probe_config(&config_client, event_probe_config.as_ref()).await
                        {
                            warn!("failed to publish retained probe config: {error}");
                        }
                    }
                    Ok(MqttEvent::Incoming(Incoming::Publish(publish))) => {
                        dispatch_probe_result(&event_sessions, &publish.topic, &publish.payload)
                            .await;
                        dispatch_probe_status(
                            &event_probe_statuses,
                            &publish.topic,
                            &publish.payload,
                        )
                        .await;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        warn!("mqtt publisher connection error: {error}");
                        rocket::tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        });

        let status_client = client.clone();
        rocket::tokio::spawn(async move {
            if let Err(error) = status_client
                .subscribe("probe/status/v1/+", QoS::AtLeastOnce)
                .await
            {
                warn!("failed to subscribe to probe status updates: {error}");
            }
        });

        info!("mqtt publisher configured for {host}:{port}");
        Self {
            client: Some(client),
            sessions,
            probe_statuses,
            probe_config,
            task_timeout_ms,
        }
    }

    pub fn task_timeout(&self) -> Duration {
        Duration::from_millis(self.task_timeout_ms)
    }

    pub async fn online_probe_ids(&self, probe_ids: &[String]) -> Vec<String> {
        let statuses = self.probe_statuses.read().await;
        probe_ids
            .iter()
            .filter(|probe_id| {
                statuses
                    .get(probe_id.as_str())
                    .is_some_and(|status| status.online)
            })
            .cloned()
            .collect()
    }

    pub async fn probe_statuses(&self) -> HashMap<String, ProbeStatusSnapshot> {
        self.probe_statuses.read().await.clone()
    }

    pub fn probe_config(&self) -> Arc<ProbeConfig> {
        self.probe_config.clone()
    }

    pub async fn subscribe_probe_results(
        &self,
        query_id: Uuid,
    ) -> Result<ProbeResultReceiver, PublishError> {
        let client = self.client.as_ref().ok_or(PublishError::NotConfigured)?;
        let query_id = query_id.to_string();
        let topic = format!("probe/results/v1/{query_id}/+");
        let receiver = {
            let mut sessions = self.sessions.write().await;
            sessions
                .entry(query_id.clone())
                .or_insert_with(|| rocket::tokio::sync::broadcast::channel(100).0)
                .subscribe()
        };

        client
            .subscribe(topic.clone(), QoS::AtLeastOnce)
            .await
            .map_err(PublishError::Subscribe)?;

        let client = client.clone();
        let sessions = self.sessions.clone();
        let cleanup_after = self.task_timeout() + Duration::from_secs(5);
        rocket::tokio::spawn(async move {
            rocket::tokio::time::sleep(cleanup_after).await;
            sessions.write().await.remove(&query_id);
            if let Err(error) = client.unsubscribe(topic).await {
                warn!("failed to unsubscribe from probe results: {error}");
            }
        });

        Ok(receiver)
    }

    pub async fn publish_probe_task(
        &self,
        query_id: Uuid,
        domain: Option<&str>,
        ip: IpAddr,
        probe_id: Option<&str>,
    ) -> Result<(), PublishError> {
        let client = self.client.as_ref().ok_or(PublishError::NotConfigured)?;
        let query_id = query_id.to_string();
        let task = ProbeTask {
            id: query_id.clone(),
            query_id: query_id.clone(),
            domain,
            ip,
            created_at: Utc::now().to_rfc3339(),
            timeout_ms: self.task_timeout_ms,
        };
        let payload = serde_json::to_vec(&task).map_err(PublishError::Serialize)?;
        let topic = match probe_id {
            Some(probe_id) => format!("probe/tasks/v1/{probe_id}/{query_id}"),
            None => format!("probe/tasks/v1/{query_id}"),
        };

        client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .map_err(PublishError::Publish)
    }
}

async fn publish_probe_config(
    client: &AsyncClient,
    config: &ProbeConfig,
) -> Result<(), PublishError> {
    let payload = serde_json::to_vec(config).map_err(PublishError::Serialize)?;

    client
        .publish("probe/config/v1", QoS::AtLeastOnce, true, payload)
        .await
        .map_err(PublishError::Publish)
}

fn load_probe_config(task_timeout_ms: u64) -> Result<ProbeConfig, PublishError> {
    let config = if let Some(path) = std::env::var_os("PROBE_CONFIG_PATH") {
        let contents = std::fs::read_to_string(path).map_err(PublishError::Config)?;
        parse_probe_hosts(&contents)?
    } else {
        parse_probe_hosts(DEFAULT_PROBE_HOSTS)?
    };
    Ok(ProbeConfig {
        version: env!("CARGO_PKG_VERSION").to_string(),
        task_timeout_ms,
        published_at: Utc::now().to_rfc3339(),
        hosts: config.hosts,
        traceroute_enabled: std::env::var("PROBE_TRACEROUTE_ENABLED")
            .ok()
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        dns_samples_per_protocol: config.dns_samples_per_protocol,
        dns_spoofing_provider_threshold: config.dns_spoofing_provider_threshold,
        dpi_probe: config.dpi_probe,
    })
}

struct ParsedProbeConfig {
    hosts: Vec<Host>,
    dns_samples_per_protocol: u8,
    dns_spoofing_provider_threshold: u8,
    dpi_probe: Option<DpiProbeConfig>,
}

fn parse_probe_hosts(contents: &str) -> Result<ParsedProbeConfig, PublishError> {
    let config: ProbeHostsFile = toml::from_str(&contents).map_err(PublishError::ConfigParse)?;

    let hosts = config
        .hosts
        .into_iter()
        .map(|host| Host {
            id: host.id,
            host: host.host,
            host_type: host.host_type,
            file_path: host.file_path,
            timeout_sec: host.timeout_sec.unwrap_or(config.timeout_sec),
            min_data: host.min_data.unwrap_or(config.min_data),
        })
        .collect();

    Ok(ParsedProbeConfig {
        hosts,
        dns_samples_per_protocol: config.dns_samples_per_protocol,
        dns_spoofing_provider_threshold: config.dns_spoofing_provider_threshold,
        dpi_probe: config.dpi_probe,
    })
}

async fn dispatch_probe_status(probe_statuses: &ProbeStatuses, topic: &str, payload: &[u8]) {
    let Some(probe_id) = parse_probe_status_topic(topic) else {
        return;
    };

    if payload.is_empty() {
        probe_statuses.write().await.remove(probe_id);
        return;
    }

    let status: ProbeStatus = match serde_json::from_slice(payload) {
        Ok(status) => status,
        Err(error) => {
            warn!("ignoring invalid probe status JSON on {topic}: {error}");
            return;
        }
    };

    if status.probe_id != probe_id {
        warn!(
            "ignoring probe status on {topic}: payload probe_id {} does not match topic",
            status.probe_id
        );
        return;
    }

    probe_statuses.write().await.insert(
        probe_id.to_string(),
        ProbeStatusSnapshot {
            online: status.online,
            version: status.version.to_string(),
            dpi_hop_v4: status.dpi_hop_v4,
            dpi_hop_v6: status.dpi_hop_v6,
        },
    );
}

async fn dispatch_probe_result(
    sessions: &Arc<rocket::tokio::sync::RwLock<HashMap<String, ProbeResultSender>>>,
    topic: &str,
    payload: &[u8],
) {
    let Some((job_id, probe_id)) = parse_probe_result_topic(topic) else {
        return;
    };

    let result: ProbeResult = match serde_json::from_slice(payload) {
        Ok(result) => result,
        Err(error) => {
            warn!("ignoring invalid probe result JSON on {topic}: {error}");
            return;
        }
    };

    let sender = sessions.read().await.get(job_id).cloned();
    if let Some(sender) = sender {
        let _ = sender.send(ProbeResultEvent {
            job_id: job_id.to_string(),
            probe_id: probe_id.to_string(),
            host_results: result.responses.unwrap_or_default(),
            target_traceroute: result.target_traceroute,
            dpi_hop: result.dpi_hop,
            dns: result.dns,
        });
    }
}

fn parse_probe_status_topic(topic: &str) -> Option<&str> {
    let mut parts = topic.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("probe"), Some("status"), Some("v1"), Some(probe_id), None) => Some(probe_id),
        _ => None,
    }
}

fn parse_probe_result_topic(topic: &str) -> Option<(&str, &str)> {
    let mut parts = topic.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("probe"), Some("results"), Some("v1"), Some(job_id), Some(probe_id), None) => {
            Some((job_id, probe_id))
        }
        _ => None,
    }
}

fn task_timeout_ms_from_env() -> u64 {
    std::env::var("MQTT_PROBE_TASK_TIMEOUT_MS")
        .ok()
        .and_then(|timeout| timeout.parse().ok())
        .unwrap_or(15_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rocket::async_test]
    async fn status_snapshot_keeps_offline_node_metadata() {
        let statuses = Arc::new(rocket::tokio::sync::RwLock::new(HashMap::new()));

        dispatch_probe_status(
            &statuses,
            "probe/status/v1/42",
            br#"{"online":false,"probe_id":"42","version":"1.2.3","dpi_hop_v4":4,"dpi_hop_v6":6}"#,
        )
        .await;

        assert_eq!(
            statuses.read().await.get("42"),
            Some(&ProbeStatusSnapshot {
                online: false,
                version: "1.2.3".to_string(),
                dpi_hop_v4: Some(4),
                dpi_hop_v6: Some(6),
            })
        );
    }

    #[rocket::async_test]
    async fn empty_retained_status_removes_snapshot() {
        let statuses = Arc::new(rocket::tokio::sync::RwLock::new(HashMap::from([(
            "42".to_string(),
            ProbeStatusSnapshot {
                online: true,
                version: "1.2.3".to_string(),
                dpi_hop_v4: None,
                dpi_hop_v6: None,
            },
        )])));

        dispatch_probe_status(&statuses, "probe/status/v1/42", b"").await;

        assert!(statuses.read().await.is_empty());
    }

    #[rocket::async_test]
    async fn mismatched_payload_id_is_ignored() {
        let statuses = Arc::new(rocket::tokio::sync::RwLock::new(HashMap::new()));

        dispatch_probe_status(
            &statuses,
            "probe/status/v1/42",
            br#"{"online":true,"probe_id":"7","version":"1.2.3"}"#,
        )
        .await;

        assert!(statuses.read().await.is_empty());
    }
}
