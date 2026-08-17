use log::{info, warn};
use reports::probe::HostType;
use reports::probe::{Host, ProbeConfig, ProbeResult, ProbeResultEvent, ProbeStatus, ProbeTask};
use rocket::serde::json::serde_json;
use rumqttc::{AsyncClient, Event as MqttEvent, Incoming, MqttOptions, QoS};
use serde::Deserialize;
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

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
    online_probes: Arc<rocket::tokio::sync::RwLock<HashSet<String>>>,
    probe_config: Arc<ProbeConfig>,
    task_timeout_ms: u64,
}

type ProbeResultSender = rocket::tokio::sync::broadcast::Sender<ProbeResultEvent>;
pub type ProbeResultReceiver = rocket::tokio::sync::broadcast::Receiver<ProbeResultEvent>;

#[derive(Deserialize)]
struct ProbeHostsFile {
    timeout_sec: u32,
    min_data: u32,
    #[serde(default)]
    control_hosts: Vec<String>,
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
        let online_probes = Arc::new(rocket::tokio::sync::RwLock::new(HashSet::new()));
        let task_timeout_ms = task_timeout_ms_from_env();
        let probe_config = Arc::new(load_probe_config(task_timeout_ms).unwrap_or_else(|error| {
            warn!("failed to load probe config: {error}");
            ProbeConfig {
                version: env!("CARGO_PKG_VERSION").to_string(),
                task_timeout_ms,
                published_at: Utc::now().to_rfc3339(),
                hosts: Vec::new(),
                traceroute_enabled: false,
                control_hosts: Vec::new(),
            }
        }));
        let admin_token = match std::env::var("MQTT_ADMIN_TOKEN") {
            Ok(token) if !token.is_empty() => token,
            _ => {
                warn!("mqtt publisher disabled: MQTT_ADMIN_TOKEN is not set");
                return Self {
                    client: None,
                    sessions,
                    online_probes,
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

        let (client, mut eventloop) = AsyncClient::new(options, 100);
        let event_sessions = sessions.clone();
        let event_online_probes = online_probes.clone();
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
                            &event_online_probes,
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
            online_probes,
            probe_config,
            task_timeout_ms,
        }
    }

    pub fn task_timeout(&self) -> Duration {
        Duration::from_millis(self.task_timeout_ms)
    }

    pub async fn online_probe_count(&self) -> usize {
        self.online_probes.read().await.len()
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
        let topic = format!("probe/tasks/v1/{query_id}");

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
        control_hosts: config.control_hosts,
    })
}

struct ParsedProbeConfig {
    hosts: Vec<Host>,
    control_hosts: Vec<String>,
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
        control_hosts: config.control_hosts,
    })
}

async fn dispatch_probe_status(
    online_probes: &Arc<rocket::tokio::sync::RwLock<HashSet<String>>>,
    topic: &str,
    payload: &[u8],
) {
    let Some(probe_id) = parse_probe_status_topic(topic) else {
        return;
    };

    if payload.is_empty() {
        online_probes.write().await.remove(probe_id);
        return;
    }

    let status: ProbeStatus = match serde_json::from_slice(payload) {
        Ok(status) => status,
        Err(error) => {
            warn!("ignoring invalid probe status JSON on {topic}: {error}");
            return;
        }
    };

    let mut online_probes = online_probes.write().await;
    if status.online {
        online_probes.insert(probe_id.to_string());
    } else {
        online_probes.remove(probe_id);
    }
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
            control_traceroute: result.control_traceroute,
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
