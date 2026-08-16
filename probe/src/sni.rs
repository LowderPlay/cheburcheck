use anyhow::{Result, bail};
use futures::future::join_all;
use log::warn;
use reports::probe::{Host, HostProbeResult, ProbeConfig, ProbeEvidence};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time;
use tokio_rustls::TlsConnector;

pub async fn check_sni(
    config: Option<&ProbeConfig>,
    domain: Option<&str>,
    timeout: Duration,
    job_id: &str,
    task_timeout_ms: u64,
) -> Result<Option<Vec<HostProbeResult>>> {
    let Some(domain) = domain else {
        return Ok(None);
    };
    let Some(config) = config else {
        bail!("no config");
    };
    let probing = join_all(config.hosts.iter().map(|host| async move {
        let probe_evidence = probe_host(host, domain).await;
        HostProbeResult {
            probe_evidence,
            host_id: host.id.clone(),
        }
    }));

    match time::timeout(timeout, probing).await {
        Ok(responses) => Ok(Some(responses)),
        Err(_) => {
            warn!("SNI checks expired for task {job_id}: timeout {task_timeout_ms}ms");
            Ok(None)
        }
    }
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
