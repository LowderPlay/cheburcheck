use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddrV4, SocketAddrV6};

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeStatus<'a> {
    pub online: bool,
    pub probe_id: &'a str,
    pub version: &'a str,
    #[serde(default)]
    pub bundle_type: Option<&'a str>,
    #[serde(default)]
    pub dpi_hop_v4: Option<u8>,
    #[serde(default)]
    pub dpi_hop_v6: Option<u8>,
    #[serde(default)]
    pub dpi_hops_v4: Vec<DpiProbeHop>,
    #[serde(default)]
    pub dpi_hops_v6: Vec<DpiProbeHop>,
}

/// One attempted DPI-probe TTL, including unsuccessful measurements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DpiProbeHop {
    pub ttl: u8,
    /// Observed ICMP response source; absent when no packet source was captured.
    #[serde(rename = "src")]
    pub router: Option<IpAddr>,
    pub outcome: DpiProbeHopOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_diagnostics: Option<TcpDiagnostics>,
}

/// Incoming TCP headers observed during one manual hop attempt. No payload is retained.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct TcpDiagnostics {
    pub packets: Vec<TcpPacketMetadata>,
    pub capture_error: Option<String>,
    pub truncated: bool,
    pub connect_error: Option<String>,
    pub send_error: Option<String>,
    pub client_hello_sent_ms: Option<u64>,
    pub junk_sent_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TcpPacketMetadata {
    /// Approximate observation time relative to the start of this hop's capture.
    pub observed_ms: u64,
    pub source: std::net::SocketAddr,
    pub destination: std::net::SocketAddr,
    /// Absent when the platform's raw IPv6 socket omits the IP header.
    pub ttl: Option<u8>,
    pub ip_id: Option<u16>,
    pub sequence: u32,
    pub acknowledgment: u32,
    pub window: u16,
    pub flags: Vec<String>,
    pub timestamp: Option<u32>,
    pub timestamp_echo: Option<u32>,
    pub options_hex: String,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DpiProbeHopOutcome {
    IcmpTimeExceeded,
    Timeout,
    TcpClosed,
    TcpAcknowledged,
}

impl DpiProbeHopOutcome {
    pub const fn invalidates_measurement(self) -> bool {
        matches!(self, Self::TcpClosed | Self::TcpAcknowledged)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeConfig {
    pub version: String,
    pub task_timeout_ms: u64,
    pub published_at: String,
    pub hosts: Vec<Host>,
    #[serde(default)]
    pub traceroute_enabled: bool,
    #[serde(default = "default_dns_samples_per_protocol")]
    pub dns_samples_per_protocol: u8,
    #[serde(default = "default_dns_spoofing_provider_threshold")]
    pub dns_spoofing_provider_threshold: u8,
    #[serde(default)]
    pub dpi_probe: Option<DpiProbeConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DpiProbeConfig {
    pub sni: String,
    pub target_v4: SocketAddrV4,
    pub target_v6: SocketAddrV6,
    pub connect_timeout_ms: u64,
    pub hop_timeout_ms: u64,
    pub max_ttl: u8,
    #[serde(default = "default_post_dpi_hop_limit")]
    pub post_dpi_hop_limit: u8,
    /// Treat TCP RST/FIN as a timeout and continue measuring later TTLs.
    #[serde(default)]
    pub accept_rst_fin_as_timeout: bool,
    /// Treat an acknowledged payload as a timeout and continue measuring later TTLs.
    #[serde(default)]
    pub accept_ack_as_timeout: bool,
}

pub const fn default_post_dpi_hop_limit() -> u8 {
    3
}

pub const fn default_dns_samples_per_protocol() -> u8 {
    3
}

pub const fn default_dns_spoofing_provider_threshold() -> u8 {
    2
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub host: String,
    pub host_type: HostType,
    pub file_path: String,
    pub timeout_sec: u32,
    pub min_data: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum HostType {
    Whitelist,
    Blacklist,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeTask<'a> {
    pub id: String,
    pub query_id: String,
    pub domain: Option<&'a str>,
    pub ip: IpAddr,
    pub created_at: String,
    pub timeout_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeResultEvent {
    pub job_id: String,
    pub probe_id: String,
    pub host_results: Vec<HostProbeResult>,
    pub target_traceroute: Option<TcpTracerouteResult>,
    pub dpi_hop: Option<u8>,
    pub dns: Option<DnsProbeResult>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub responses: Option<Vec<HostProbeResult>>,
    pub target_traceroute: Option<TcpTracerouteResult>,
    #[serde(default)]
    pub dpi_hop: Option<u8>,
    #[serde(default)]
    pub dns: Option<DnsProbeResult>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DnsProbeResult {
    pub spoofing_detected: bool,
    #[serde(default)]
    pub suspicious_provider_count: u8,
    #[serde(default)]
    pub verdict_threshold: u8,
    #[serde(default)]
    pub samples_per_protocol: u8,
    pub observations: Vec<DnsObservation>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DnsObservation {
    pub provider: String,
    pub protocol: DnsProtocol,
    pub outcome: DnsOutcome,
    #[serde(default)]
    pub suspected_spoofing: bool,
    #[serde(default)]
    pub metadata: DnsResponseMetadata,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DnsResponseMetadata {
    pub response_codes: Vec<String>,
    pub ipv4_count: u16,
    pub ipv6_count: u16,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum DnsProtocol {
    Udp,
    Tcp,
    Doh,
    Dot,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum DnsOutcome {
    Answer { addresses: Vec<IpAddr> },
    NoRecords,
    Error { message: String },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TcpTracerouteResult {
    pub target: IpAddr,
    pub result: TcpTracerouteOutcome,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProbeCommand {
    Traceroute {
        target: IpAddr,
        #[serde(default = "default_manual_max_hops")]
        max_hops: u8,
    },
    ResubscribeTasks,
    RemeasureDpiHop,
    SniTraceroute {
        host: String,
        sni: String,
        #[serde(default = "default_manual_max_hops")]
        max_hops: u8,
    },
}

pub const fn default_manual_max_hops() -> u8 {
    30
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProbeCommandResult {
    Traceroute {
        target: IpAddr,
        hops: Vec<ManualTracerouteHop>,
    },
    ResubscribeTasks {
        requested: bool,
    },
    RemeasureDpiHop {
        dpi_hop_v4: Option<u8>,
        dpi_hop_v6: Option<u8>,
        dpi_hops_v4: Vec<DpiProbeHop>,
        dpi_hops_v6: Vec<DpiProbeHop>,
    },
    SniTraceroute {
        host: String,
        sni: String,
        target: std::net::SocketAddr,
        dpi_hop: Option<u8>,
        hops: Vec<DpiProbeHop>,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ManualTracerouteHop {
    pub ttl: u8,
    pub address: Option<IpAddr>,
    pub reverse_names: Vec<String>,
    pub outcome: ManualTracerouteOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_diagnostics: Option<TcpDiagnostics>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualTracerouteOutcome {
    IcmpTimeExceeded,
    Rst,
    Connected,
    Timeout,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TcpTracerouteOutcome {
    Rst { hop: u8 },
    Connected { hop: u8 },
    IcmpTimeExceeded { hop: u8 },
    Timeout,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HostProbeResult {
    pub host_id: String,
    pub probe_evidence: ProbeEvidence,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProbeEvidence {
    ConnectionError,
    ClientHello,
    DataTimeout { bytes: u32 },
    Good,
}

/// Manual SNI traces accept a bare hostname or IP and a DNS SNI, on port 443.
pub fn validate_sni_traceroute(host: &str, sni: &str) -> Result<(), &'static str> {
    fn dns_name(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 253
            && value
                .strip_suffix('.')
                .unwrap_or(value)
                .split('.')
                .all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
    }
    if host.parse::<IpAddr>().is_err() && !dns_name(host) {
        return Err("host must be a bare hostname or IP address");
    }
    if sni.parse::<IpAddr>().is_ok() || !dns_name(sni) {
        return Err("sni must be a DNS name");
    }
    Ok(())
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn validates_separate_host_and_sni() {
        for host in ["example.com", "192.0.2.1", "2001:db8::1"] {
            assert!(validate_sni_traceroute(host, "blocked.example").is_ok());
        }
        for host in [
            "",
            "https://example.com",
            "example.com:443",
            "bad host",
            "-invalid.example",
        ] {
            assert!(validate_sni_traceroute(host, "blocked.example").is_err());
        }
        for sni in [
            "",
            "192.0.2.1",
            "bad/name",
            "bad..name",
            "bad_.name",
            "bad...",
        ] {
            assert!(validate_sni_traceroute("example.com", sni).is_err());
        }
    }
}
