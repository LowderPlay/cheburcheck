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
