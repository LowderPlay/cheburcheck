use anyhow::{Context, Result, bail};
use clap::Parser;
use futures::future::join_all;
use log::{error, info, warn};
use reports::probe::{Host, HostProbeResult, ProbeConfig, ProbeEvidence, ProbeStatus, ProbeTask};
use rumqttc::{
    AsyncClient, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS, Transport,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time;
use tokio_rustls::TlsConnector;

const CONFIG_TOPIC: &str = "probe/config/v1";

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
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    if args.max_concurrent_tasks == 0 {
        bail!("max_concurrent_tasks must be greater than zero");
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

async fn update_config(config: &Arc<RwLock<Option<ProbeConfig>>>, payload: &[u8]) -> Result<()> {
    let value = serde_json::from_slice(payload).context("decode probe config")?;
    *config.write().await = Some(value);
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
    config: &Arc<RwLock<Option<ProbeConfig>>>,
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

    let config = config.read().await.clone();
    let Some(config) = config else {
        bail!("no config");
    };
    let result_topic = format!("probe/results/v1/{job_id}/{}", args.probe_id);
    let probing = join_all(config.hosts.into_iter().map(|host| {
        let target = task.target.to_string();
        async move {
            let probe_evidence = probe_host(&host, &target).await;
            HostProbeResult {
                probe_evidence,
                host_id: host.id,
            }
        }
    }));
    let result = match time::timeout(remaining, probing).await {
        Ok(result) => result,
        Err(_) => {
            warn!(
                "dropping expired task {job_id}: timeout {}ms",
                task.timeout_ms
            );
            return Ok(());
        }
    };

    client
        .publish(
            result_topic,
            QoS::AtLeastOnce,
            false,
            serde_json::to_vec(&result)?,
        )
        .await
        .context("publish probe result")
}

async fn probe_host(host: &Host, target: &str) -> ProbeEvidence {
    let timeout = Duration::from_secs(host.timeout_sec as u64);
    let tcp = match time::timeout(timeout, TcpStream::connect((host.host.as_str(), 443))).await {
        Ok(Ok(tcp)) => tcp,
        Ok(Err(_)) | Err(_) => return ProbeEvidence::ConnectionError,
    };

    let tls_config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(tls_config));

    let server_name = match ServerName::try_from(target.to_string()) {
        Ok(server_name) => server_name,
        Err(_) => return ProbeEvidence::ClientHello,
    };

    let mut tls = match time::timeout(timeout, connector.connect(server_name, tcp)).await {
        Ok(Ok(tls)) => tls,
        Ok(Err(_)) | Err(_) => return ProbeEvidence::ClientHello,
    };

    let request = format!(
        "GET /{} HTTP/1.1\r\nHost: {}\r\nUser-Agent: cheburcheck-probe/{}\r\nRange: bytes=0-{}\r\nConnection: close\r\n\r\n",
        host.file_path.trim_start_matches('/'),
        target,
        env!("CARGO_PKG_VERSION"),
        host.min_data.saturating_sub(1)
    );

    if !matches!(
        time::timeout(timeout, tls.write_all(request.as_bytes())).await,
        Ok(Ok(()))
    ) {
        return ProbeEvidence::ClientHello;
    }

    let mut received = 0u32;
    let mut headers_done = false;
    let mut pending = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match time::timeout(timeout, tls.read(&mut buffer)).await {
            Ok(Ok(0)) | Err(_) => {
                return if received >= host.min_data {
                    ProbeEvidence::Good
                } else {
                    ProbeEvidence::DataTimeout { bytes: received }
                };
            }
            Ok(Ok(bytes)) => {
                add_response_body_bytes(
                    &buffer[..bytes],
                    &mut pending,
                    &mut headers_done,
                    &mut received,
                );
                if received >= host.min_data {
                    return ProbeEvidence::Good;
                }
            }
            Ok(Err(_)) => {
                return if received >= host.min_data {
                    ProbeEvidence::Good
                } else {
                    ProbeEvidence::DataTimeout { bytes: received }
                };
            }
        }
    }
}

fn add_response_body_bytes(
    chunk: &[u8],
    pending: &mut Vec<u8>,
    headers_done: &mut bool,
    received: &mut u32,
) {
    if *headers_done {
        *received = received.saturating_add(chunk.len() as u32);
        return;
    }

    pending.extend_from_slice(chunk);
    if let Some(body_start) = pending.windows(4).position(|window| window == b"\r\n\r\n") {
        *headers_done = true;
        let body_bytes = pending.len().saturating_sub(body_start + 4);
        *received = received.saturating_add(body_bytes as u32);
        pending.clear();
    }
}

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}
