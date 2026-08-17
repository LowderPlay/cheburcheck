use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeStatus<'a> {
    pub online: bool,
    pub probe_id: &'a str,
    pub version: &'a str,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeConfig {
    pub version: String,
    pub task_timeout_ms: u64,
    pub published_at: String,
    pub hosts: Vec<Host>,
    #[serde(default)]
    pub traceroute_enabled: bool,
    #[serde(default)]
    pub control_hosts: Vec<String>,
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
    pub control_traceroute: Option<TcpTracerouteResult>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub responses: Option<Vec<HostProbeResult>>,
    pub target_traceroute: Option<TcpTracerouteResult>,
    pub control_traceroute: Option<TcpTracerouteResult>,
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
