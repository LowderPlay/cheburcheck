mod dns;
mod dpi_hop;
mod sni;
mod traceroute;
mod update;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use log::{debug, error, info, warn};
use reports::probe::{DpiProbeConfig, ProbeConfig, ProbeResult, ProbeStatus, ProbeTask};
use rumqttc::{
    AsyncClient, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS, Transport,
};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const CONFIG_TOPIC: &str = "probe/config/v1";
const UPDATE_TOPIC: &str = "probe/update/v1";
const UPDATE_REQUEST_COMMAND: &str = "/usr/libexec/cheburprobe-request-update";
const MQTT_MAX_PACKET_SIZE: usize = 1024 * 1024;

#[derive(Clone)]
struct LoadedProbeConfig {
    config: ProbeConfig,
    dpi_hop_v4: Option<u8>,
    dpi_hop_v6: Option<u8>,
}

#[derive(Clone, Copy, Default)]
struct DpiHops {
    v4: Option<u8>,
    v6: Option<u8>,
}

#[derive(Parser, Debug, Clone)]
#[command(
    author,
    version,
    about = "Dynamic probing daemon",
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, env = "MQTT_HOST", default_value = "wss://cheburcheck.ru/mqtt")]
    mqtt_host: String,

    #[arg(long, env = "MQTT_PORT", default_value_t = 443)]
    mqtt_port: u16,

    #[arg(long, env = "MQTT_CONNECTION_TIMEOUT_SECS", default_value_t = 30)]
    mqtt_connection_timeout_secs: u64,

    #[arg(long, env = "PROBE_ID")]
    probe_id: Option<String>,

    #[arg(long, env = "PROBE_TOKEN")]
    probe_token: Option<String>,

    #[arg(long, env = "MAX_CONCURRENT_TASKS", default_value_t = 8)]
    max_concurrent_tasks: usize,

    #[arg(long, env = "TRACEROUTE_RETRIES", default_value_t = 3)]
    traceroute_retries: u8,
}

#[derive(Debug, Clone)]
struct Args {
    mqtt_host: String,
    mqtt_port: u16,
    mqtt_connection_timeout_secs: u64,
    probe_id: String,
    probe_token: String,
    max_concurrent_tasks: usize,
    traceroute_retries: u8,
    bundle_type: &'static str,
}

impl Cli {
    fn into_daemon_args(self) -> Result<Args> {
        Ok(Args {
            mqtt_host: self.mqtt_host,
            mqtt_port: self.mqtt_port,
            mqtt_connection_timeout_secs: self.mqtt_connection_timeout_secs,
            probe_id: self
                .probe_id
                .context("--probe-id or PROBE_ID is required when running the probe")?,
            probe_token: self
                .probe_token
                .context("--probe-token or PROBE_TOKEN is required when running the probe")?,
            max_concurrent_tasks: self.max_concurrent_tasks,
            traceroute_retries: self.traceroute_retries,
            bundle_type: update::bundle_type().context("failed to detect probe bundle type")?,
        })
    }
}

#[derive(Subcommand, Debug, Clone)]
enum Command {
    /// Update Cheburprobe from the trusted update server.
    Update,
}

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("failed to install the rustls ring crypto provider"))?;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    if matches!(cli.command, Some(Command::Update)) {
        return update::run().await;
    }
    let args = cli.into_daemon_args()?;
    if args.max_concurrent_tasks == 0 {
        bail!("max_concurrent_tasks must be greater than zero");
    }
    if args.traceroute_retries == 0 {
        bail!("traceroute_retries must be greater than zero");
    }

    let status_topic = format!("probe/status/v1/{}", args.probe_id);
    let offline_status = serde_json::to_vec(&ProbeStatus {
        online: false,
        probe_id: &args.probe_id,
        version: env!("CARGO_PKG_VERSION"),
        bundle_type: Some(args.bundle_type),
        dpi_hop_v4: None,
        dpi_hop_v6: None,
    })?;

    let mut options = MqttOptions::new(&args.probe_id, &args.mqtt_host, args.mqtt_port);
    options.set_transport(mqtt_transport(&args.mqtt_host)?);
    options.set_credentials("probe", &args.probe_token);
    options.set_keep_alive(Duration::from_secs(10));
    options.set_max_packet_size(MQTT_MAX_PACKET_SIZE, MQTT_MAX_PACKET_SIZE);
    options.set_last_will(LastWill::new(
        status_topic.clone(),
        offline_status,
        QoS::AtLeastOnce,
        true,
    ));

    let (client, mut eventloop) = AsyncClient::new(options, 100);
    let mqtt_updates_enabled = Path::new(UPDATE_REQUEST_COMMAND).is_file();
    let config = Arc::new(RwLock::new(None));
    let task_semaphore = Arc::new(tokio::sync::Semaphore::new(args.max_concurrent_tasks));
    let mut network_options = NetworkOptions::new();
    network_options.set_connection_timeout(args.mqtt_connection_timeout_secs);
    eventloop.set_network_options(network_options);

    wait_for_connection(&mut eventloop).await;
    publish_status(&client, &status_topic, &args, true, DpiHops::default()).await?;
    client.subscribe(CONFIG_TOPIC, QoS::AtLeastOnce).await?;
    if mqtt_updates_enabled {
        subscribe_to_update_requests(&client, &args.probe_id).await?;
    } else {
        debug!("MQTT-triggered updates are disabled for this standalone installation");
    }
    client
        .subscribe("probe/tasks/v1/+", QoS::AtLeastOnce)
        .await?;
    client
        .subscribe(
            format!("probe/tasks/v1/{}/+", args.probe_id),
            QoS::AtLeastOnce,
        )
        .await?;

    info!(
        "probe {} connected over WebSocket to {}",
        args.probe_id, args.mqtt_host
    );

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                if mqtt_updates_enabled && is_update_topic(&publish.topic, &args.probe_id) {
                    if publish.retain {
                        warn!("ignoring retained update request on {}", publish.topic);
                    } else {
                        request_update_check();
                    }
                } else if publish.topic == CONFIG_TOPIC {
                    spawn_config_update(
                        client.clone(),
                        status_topic.clone(),
                        args.clone(),
                        config.clone(),
                        publish.payload.to_vec(),
                    );
                } else {
                    let client = client.clone();
                    let args = args.clone();
                    let config = config.clone();
                    let semaphore = task_semaphore.clone();
                    let topic = publish.topic;
                    let payload = publish.payload.to_vec();
                    let received_at = Instant::now();

                    tokio::spawn(async move {
                        let task: ProbeTask =
                            match serde_json::from_slice(&payload).context("decode probe task") {
                                Ok(task) => task,
                                Err(error) => {
                                    warn!("failed to decode task on {topic}: {error}");
                                    return;
                                }
                            };

                        let permit = match semaphore.acquire_owned().await {
                            Ok(permit) => permit,
                            Err(error) => {
                                warn!("failed to acquire task permit: {error}");
                                return;
                            }
                        };

                        if let Err(error) =
                            handle_task(&client, &args, &config, &topic, task, received_at).await
                        {
                            warn!("failed to handle task on {topic}: {error}");
                        }

                        drop(permit);
                    });
                }
            }
            Ok(_) => {}
            Err(error) => {
                error!("mqtt connection error: {error}");
                wait_for_connection(&mut eventloop).await;
                let dpi_hops =
                    config
                        .read()
                        .await
                        .as_ref()
                        .map_or_else(DpiHops::default, |config| DpiHops {
                            v4: config.dpi_hop_v4,
                            v6: config.dpi_hop_v6,
                        });
                publish_status(&client, &status_topic, &args, true, dpi_hops).await?;
                client.subscribe(CONFIG_TOPIC, QoS::AtLeastOnce).await?;
                if mqtt_updates_enabled {
                    subscribe_to_update_requests(&client, &args.probe_id).await?;
                }
                client
                    .subscribe("probe/tasks/v1/+", QoS::AtLeastOnce)
                    .await?;
                client
                    .subscribe(
                        format!("probe/tasks/v1/{}/+", args.probe_id),
                        QoS::AtLeastOnce,
                    )
                    .await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

fn spawn_config_update(
    client: AsyncClient,
    status_topic: String,
    args: Args,
    config: Arc<RwLock<Option<LoadedProbeConfig>>>,
    payload: Vec<u8>,
) {
    tokio::spawn(async move {
        match update_config(&config, &payload).await {
            Ok(dpi_hops) => {
                if let Err(error) =
                    publish_status(&client, &status_topic, &args, true, dpi_hops).await
                {
                    warn!("failed to publish probe status with DPI hop: {error}");
                }
            }
            Err(error) => warn!("failed to update probe config: {error}"),
        }
    });
}

async fn subscribe_to_update_requests(client: &AsyncClient, probe_id: &str) -> Result<()> {
    client.subscribe(UPDATE_TOPIC, QoS::AtLeastOnce).await?;
    client
        .subscribe(format!("{UPDATE_TOPIC}/{probe_id}"), QoS::AtLeastOnce)
        .await?;
    Ok(())
}

fn is_update_topic(topic: &str, probe_id: &str) -> bool {
    topic == UPDATE_TOPIC || topic == format!("{UPDATE_TOPIC}/{probe_id}")
}

fn request_update_check() {
    tokio::spawn(async {
        match tokio::process::Command::new(UPDATE_REQUEST_COMMAND)
            .status()
            .await
        {
            Ok(status) if status.success() => info!("requested an update check over MQTT"),
            Ok(status) => warn!("update request command exited with {status}"),
            Err(error) => warn!("failed to request an update check: {error}"),
        }
    });
}

fn mqtt_transport(mqtt_host: &str) -> Result<Transport> {
    if mqtt_host.starts_with("wss://") {
        Ok(Transport::wss_with_default_config())
    } else if mqtt_host.starts_with("ws://") {
        Ok(Transport::Ws)
    } else {
        bail!("MQTT_HOST must start with ws:// or wss://");
    }
}

async fn update_config(
    config: &Arc<RwLock<Option<LoadedProbeConfig>>>,
    payload: &[u8],
) -> Result<DpiHops> {
    let value: ProbeConfig = serde_json::from_slice(payload).context("decode probe config")?;
    if value.dns_samples_per_protocol == 0 {
        bail!("dns_samples_per_protocol must be greater than zero");
    }
    if !(1..=4).contains(&value.dns_spoofing_provider_threshold) {
        bail!("dns_spoofing_provider_threshold must be between 1 and 4");
    }
    validate_dpi_probe_config(value.dpi_probe.as_ref())?;
    let dpi_hops = measure_dpi_hops(value.dpi_probe.as_ref()).await;
    let loaded = LoadedProbeConfig {
        config: value,
        dpi_hop_v4: dpi_hops.v4,
        dpi_hop_v6: dpi_hops.v6,
    };
    debug!(
        "measured DPI hops: IPv4={:?}, IPv6={:?}",
        loaded.dpi_hop_v4, loaded.dpi_hop_v6
    );
    *config.write().await = Some(loaded);
    info!("updated retained probe config");
    Ok(dpi_hops)
}

fn validate_dpi_probe_config(config: Option<&DpiProbeConfig>) -> Result<()> {
    let Some(config) = config else {
        return Ok(());
    };
    if config.sni.trim().is_empty() {
        bail!("dpi_probe.sni must not be empty");
    }
    if config.target_v4.port() == 0 {
        bail!("dpi_probe.target_v4 port must be greater than zero");
    }
    if config.target_v6.port() == 0 {
        bail!("dpi_probe.target_v6 port must be greater than zero");
    }
    if config.connect_timeout_ms == 0 {
        bail!("dpi_probe.connect_timeout_ms must be greater than zero");
    }
    if config.hop_timeout_ms == 0 {
        bail!("dpi_probe.hop_timeout_ms must be greater than zero");
    }
    if config.max_ttl == 0 {
        bail!("dpi_probe.max_ttl must be greater than zero");
    }
    if config.post_dpi_hop_limit == 0 {
        bail!("dpi_probe.post_dpi_hop_limit must be greater than zero");
    }
    Ok(())
}

async fn measure_dpi_hops(config: Option<&DpiProbeConfig>) -> DpiHops {
    let Some(config) = config else {
        debug!("DPI hop measurement is not configured");
        return DpiHops::default();
    };
    let common = |target| dpi_hop::DpiHopProbeConfig {
        target,
        control_sni: config.sni.clone(),
        max_ttl: config.max_ttl,
        connect_timeout: Duration::from_millis(config.connect_timeout_ms),
        hop_timeout: Duration::from_millis(config.hop_timeout_ms),
    };
    let (v4, v6) = tokio::join!(
        measure_dpi_hop(common(config.target_v4.into())),
        measure_dpi_hop(common(config.target_v6.into())),
    );
    DpiHops { v4, v6 }
}

async fn measure_dpi_hop(config: dpi_hop::DpiHopProbeConfig) -> Option<u8> {
    let target = config.target;
    match dpi_hop::detect_dpi_hop(config).await {
        Ok(result) => dpi_hop_from_result(&result),
        Err(error) => {
            warn!("failed to measure DPI hop for {target}: {error}");
            None
        }
    }
}

fn dpi_hop_from_result(result: &dpi_hop::DpiHopProbeResult) -> Option<u8> {
    debug!(
        "DPI probe completed: target={}, local={}, ClientHello={} bytes",
        result.target, result.local_addr, result.client_hello_bytes
    );
    for hop in &result.hops {
        debug!(
            "DPI probe {} TTL {}: router={:?}, outcome={:?}",
            result.target, hop.ttl, hop.router, hop.outcome
        );
    }
    if let Some(closed_hop) = result
        .hops
        .iter()
        .find(|hop| hop.outcome == dpi_hop::DpiHopProbeHopOutcome::TcpClosed)
    {
        warn!(
            "DPI hop measurement for {} is invalid: TCP connection closed at TTL {}",
            result.target, closed_hop.ttl
        );
        return None;
    }
    result.max_icmp_time_exceeded_ttl
}

async fn wait_for_connection(eventloop: &mut rumqttc::EventLoop) {
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                info!("mqtt connection established");
                return;
            }
            Ok(event) => {
                info!("mqtt event before connection: {event:?}");
            }
            Err(error) => {
                error!("mqtt connection error while waiting for CONNACK: {error}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn publish_status(
    client: &AsyncClient,
    topic: &str,
    args: &Args,
    online: bool,
    dpi_hops: DpiHops,
) -> Result<()> {
    let payload = serde_json::to_vec(&ProbeStatus {
        online,
        probe_id: &args.probe_id,
        version: env!("CARGO_PKG_VERSION"),
        bundle_type: Some(args.bundle_type),
        dpi_hop_v4: dpi_hops.v4,
        dpi_hop_v6: dpi_hops.v6,
    })?;

    client
        .publish(topic, QoS::AtLeastOnce, true, payload)
        .await
        .context("publish probe status")
}

async fn handle_task(
    client: &AsyncClient,
    args: &Args,
    config: &Arc<RwLock<Option<LoadedProbeConfig>>>,
    topic: &str,
    task: ProbeTask<'_>,
    received_at: Instant,
) -> Result<()> {
    let job_id = probe_task_job_id(topic).unwrap_or(&task.id);
    let timeout = Duration::from_millis(task.timeout_ms);
    let Some(remaining) = timeout.checked_sub(received_at.elapsed()) else {
        warn!(
            "dropping expired queued task {job_id}: timeout {}ms",
            task.timeout_ms
        );
        return Ok(());
    };

    let result_topic = format!("probe/results/v1/{job_id}/{}", args.probe_id);
    let config = config.read().await.clone();
    let traceroute_enabled = config
        .as_ref()
        .is_some_and(|config| config.config.traceroute_enabled);
    let dpi_hop = config.as_ref().and_then(|config| match task.ip {
        IpAddr::V4(_) => config.dpi_hop_v4,
        IpAddr::V6(_) => config.dpi_hop_v6,
    });
    let traceroute_range = config.as_ref().and_then(|config| {
        let hop_limit = config.config.dpi_probe.as_ref()?.post_dpi_hop_limit;
        dpi_hop?
            .checked_add(1)
            .map(|start_hop| (start_hop, hop_limit))
    });
    let sni_check = sni::check_sni(
        config.as_ref().map(|config| &config.config),
        task.domain,
        remaining,
        job_id,
        task.timeout_ms,
    );
    let target_traceroute = async {
        match (traceroute_enabled, traceroute_range) {
            (true, Some((start_hop, hop_limit))) => {
                traceroute::tcp_traceroute(task.ip, start_hop, hop_limit, args.traceroute_retries)
                    .await
            }
            _ => None,
        }
    };
    let dns_samples_per_protocol = config
        .as_ref()
        .map_or_else(reports::probe::default_dns_samples_per_protocol, |config| {
            config.config.dns_samples_per_protocol
        });
    let dns_spoofing_provider_threshold = config.as_ref().map_or_else(
        reports::probe::default_dns_spoofing_provider_threshold,
        |config| config.config.dns_spoofing_provider_threshold,
    );
    let dns_check = dns::check_dns(
        task.domain,
        remaining,
        dns_samples_per_protocol,
        dns_spoofing_provider_threshold,
    );
    let (responses, target_traceroute, dns) = tokio::join!(sni_check, target_traceroute, dns_check);
    let responses = responses?;

    client
        .publish(
            result_topic,
            QoS::AtLeastOnce,
            false,
            serde_json::to_vec(&ProbeResult {
                responses,
                target_traceroute,
                dpi_hop,
                dns,
            })?,
        )
        .await
        .context("publish probe result")
}

fn probe_task_job_id(topic: &str) -> Option<&str> {
    let parts = topic.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["probe", "tasks", "v1", job_id] if !job_id.is_empty() => Some(job_id),
        ["probe", "tasks", "v1", _recipient, job_id] if !job_id.is_empty() => Some(job_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_update_subcommand_without_daemon_arguments() {
        let args = Cli::try_parse_from(["cheburprobe", "update"]).unwrap();
        assert!(matches!(args.command, Some(Command::Update)));
    }

    #[test]
    fn preserves_daemon_invocation_without_a_subcommand() {
        let args =
            Cli::try_parse_from(["cheburprobe", "--probe-id", "42", "--probe-token", "secret"])
                .unwrap()
                .into_daemon_args()
                .unwrap();
        assert_eq!(args.probe_id, "42");
        assert_eq!(args.probe_token, "secret");
    }

    #[test]
    fn extracts_job_id_from_legacy_global_and_individual_topics() {
        assert_eq!(probe_task_job_id("probe/tasks/v1/job-1"), Some("job-1"));
        assert_eq!(probe_task_job_id("probe/tasks/v1/42/job-2"), Some("job-2"));
        assert_eq!(probe_task_job_id("probe/tasks/v1"), None);
        assert_eq!(probe_task_job_id("probe/tasks/v1/42/job-2/extra"), None);
    }

    #[test]
    fn recognizes_global_and_individual_update_topics() {
        assert!(is_update_topic("probe/update/v1", "42"));
        assert!(is_update_topic("probe/update/v1/42", "42"));
        assert!(!is_update_topic("probe/update/v1/7", "42"));
        assert!(!is_update_topic("probe/update/v1/42/extra", "42"));
    }

    #[test]
    fn decodes_separate_dpi_targets() {
        let config: ProbeConfig = serde_json::from_value(serde_json::json!({
            "version": "1",
            "task_timeout_ms": 15_000,
            "published_at": "2026-08-21T00:00:00Z",
            "hosts": [],
            "dpi_probe": {
                "sni": "example.com",
                "target_v4": "203.0.113.10:443",
                "target_v6": "[2001:db8::10]:443",
                "connect_timeout_ms": 5_000,
                "hop_timeout_ms": 1_000,
                "max_ttl": 15
            }
        }))
        .unwrap();
        let dpi = config.dpi_probe.unwrap();

        assert_eq!(dpi.target_v4.to_string(), "203.0.113.10:443");
        assert_eq!(dpi.target_v6.to_string(), "[2001:db8::10]:443");
        assert_eq!(dpi.post_dpi_hop_limit, 3);
    }

    #[test]
    fn status_payload_contains_separate_dpi_hops() {
        let status = ProbeStatus {
            online: true,
            probe_id: "probe-1",
            version: "1.0.0",
            bundle_type: Some("debian"),
            dpi_hop_v4: Some(4),
            dpi_hop_v6: Some(6),
        };
        let value = serde_json::to_value(status).unwrap();

        assert_eq!(value["dpi_hop_v4"], 4);
        assert_eq!(value["dpi_hop_v6"], 6);
        assert_eq!(value["bundle_type"], "debian");
    }

    #[test]
    fn tcp_closed_invalidates_only_its_measurement() {
        let result = dpi_hop::DpiHopProbeResult {
            target: "[2001:db8::10]:443".parse().unwrap(),
            local_addr: "[2001:db8::1]:45000".parse().unwrap(),
            client_hello_bytes: 256,
            max_icmp_time_exceeded_ttl: Some(4),
            hops: vec![dpi_hop::DpiHopProbeHop {
                ttl: 5,
                router: None,
                outcome: dpi_hop::DpiHopProbeHopOutcome::TcpClosed,
            }],
        };

        assert_eq!(dpi_hop_from_result(&result), None);
    }
}
