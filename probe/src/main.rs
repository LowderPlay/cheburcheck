mod sni;
mod traceroute;

use anyhow::{Context, Result, bail};
use clap::Parser;
use log::{error, info, warn};
use rand::seq::SliceRandom;
use reports::probe::{ProbeConfig, ProbeResult, ProbeStatus, ProbeTask};
use rumqttc::{
    AsyncClient, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS, Transport,
};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const CONFIG_TOPIC: &str = "probe/config/v1";

#[derive(Clone)]
struct LoadedProbeConfig {
    config: ProbeConfig,
    control_hosts_v4: Vec<Ipv4Addr>,
    control_hosts_v6: Vec<Ipv6Addr>,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Dynamic probing daemon")]
struct Args {
    #[arg(long, env = "MQTT_HOST", default_value = "wss://cheburcheck.ru/mqtt")]
    mqtt_host: String,

    #[arg(long, env = "MQTT_PORT", default_value_t = 443)]
    mqtt_port: u16,

    #[arg(long, env = "MQTT_CONNECTION_TIMEOUT_SECS", default_value_t = 30)]
    mqtt_connection_timeout_secs: u64,

    #[arg(long, env = "PROBE_ID")]
    probe_id: String,

    #[arg(long, env = "PROBE_TOKEN")]
    probe_token: String,

    #[arg(long, env = "MAX_CONCURRENT_TASKS", default_value_t = 8)]
    max_concurrent_tasks: usize,

    #[arg(long, env = "TRACEROUTE_MAX_HOPS", default_value_t = 5)]
    traceroute_max_hops: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    if args.max_concurrent_tasks == 0 {
        bail!("max_concurrent_tasks must be greater than zero");
    }
    if args.traceroute_max_hops == 0 {
        bail!("traceroute_max_hops must be greater than zero");
    }

    let status_topic = format!("probe/status/v1/{}", args.probe_id);
    let offline_status = serde_json::to_vec(&ProbeStatus {
        online: false,
        probe_id: &args.probe_id,
        version: env!("CARGO_PKG_VERSION"),
    })?;

    let mut options = MqttOptions::new(&args.probe_id, &args.mqtt_host, args.mqtt_port);
    options.set_transport(mqtt_transport(&args.mqtt_host)?);
    options.set_credentials("probe", &args.probe_token);
    options.set_keep_alive(Duration::from_secs(10));
    options.set_last_will(LastWill::new(
        status_topic.clone(),
        offline_status,
        QoS::AtLeastOnce,
        true,
    ));

    let (client, mut eventloop) = AsyncClient::new(options, 100);
    let config = Arc::new(RwLock::new(None));
    let task_semaphore = Arc::new(tokio::sync::Semaphore::new(args.max_concurrent_tasks));
    let mut network_options = NetworkOptions::new();
    network_options.set_connection_timeout(args.mqtt_connection_timeout_secs);
    eventloop.set_network_options(network_options);

    wait_for_connection(&mut eventloop).await;
    publish_status(&client, &status_topic, &args, true).await?;
    client.subscribe(CONFIG_TOPIC, QoS::AtLeastOnce).await?;
    client
        .subscribe("probe/tasks/v1/+", QoS::AtLeastOnce)
        .await?;

    info!(
        "probe {} connected over WebSocket to {}",
        args.probe_id, args.mqtt_host
    );

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                if publish.topic == CONFIG_TOPIC {
                    if let Err(error) = update_config(&config, &publish.payload).await {
                        warn!("failed to update probe config: {error}");
                    }
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
                publish_status(&client, &status_topic, &args, true).await?;
                client.subscribe(CONFIG_TOPIC, QoS::AtLeastOnce).await?;
                client
                    .subscribe("probe/tasks/v1/+", QoS::AtLeastOnce)
                    .await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
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
) -> Result<()> {
    let value: ProbeConfig = serde_json::from_slice(payload).context("decode probe config")?;
    let mut control_hosts_v4 = HashSet::new();
    let mut control_hosts_v6 = HashSet::new();
    for domain in &value.control_hosts {
        match tokio::net::lookup_host((domain.as_str(), 443)).await {
            Ok(addresses) => {
                for address in addresses {
                    match address.ip() {
                        IpAddr::V4(address) => {
                            control_hosts_v4.insert(address);
                        }
                        IpAddr::V6(address) => {
                            control_hosts_v6.insert(address);
                        }
                    }
                }
            }
            Err(error) => warn!("failed to resolve control host {domain}: {error}"),
        }
    }
    *config.write().await = Some(LoadedProbeConfig {
        config: value,
        control_hosts_v4: control_hosts_v4.into_iter().collect(),
        control_hosts_v6: control_hosts_v6.into_iter().collect(),
    });
    info!("updated retained probe config");
    Ok(())
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
) -> Result<()> {
    let payload = serde_json::to_vec(&ProbeStatus {
        online,
        probe_id: &args.probe_id,
        version: env!("CARGO_PKG_VERSION"),
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
    let job_id = topic
        .strip_prefix("probe/tasks/v1/")
        .filter(|id| !id.is_empty())
        .unwrap_or(&task.id);
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
    let control_target = config.as_ref().and_then(|config| {
        let mut rng = rand::thread_rng();
        match task.ip {
            IpAddr::V4(_) => config
                .control_hosts_v4
                .choose(&mut rng)
                .copied()
                .map(IpAddr::V4),
            IpAddr::V6(_) => config
                .control_hosts_v6
                .choose(&mut rng)
                .copied()
                .map(IpAddr::V6),
        }
    });
    let sni_check = sni::check_sni(
        config.as_ref().map(|config| &config.config),
        task.domain,
        remaining,
        job_id,
        task.timeout_ms,
    );
    let target_traceroute = traceroute::tcp_traceroute(task.ip, args.traceroute_max_hops);
    let control_traceroute = async {
        match control_target {
            Some(target) => traceroute::tcp_traceroute(target, args.traceroute_max_hops).await,
            None => None,
        }
    };
    let (responses, target_traceroute, control_traceroute) =
        tokio::join!(sni_check, target_traceroute, control_traceroute);
    let responses = responses?;

    client
        .publish(
            result_topic,
            QoS::AtLeastOnce,
            false,
            serde_json::to_vec(&ProbeResult {
                responses,
                target_traceroute,
                control_traceroute,
            })?,
        )
        .await
        .context("publish probe result")
}
