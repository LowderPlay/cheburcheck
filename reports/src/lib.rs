use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::net::IpAddr;

#[derive(Debug, Serialize, Deserialize)]
pub struct AgencyReport {
    pub version: String,
    pub config: ReporterConfig,
    pub data: HashMap<String, Evidence>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Evidence {
    Ok,
    Blocked,
    ConnectError,
    Error,
}

impl Display for Evidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Evidence::Ok => "ok",
            Evidence::Blocked => "blocked",
            Evidence::ConnectError => "connect_error",
            Evidence::Error => "unknown_error",
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReporterConfig {
    pub http: bool,
    pub tx_junk: bool,
    pub ip: IpAddr,
    pub path: String,
    pub retry_count: usize,
    pub timeout_secs: u64,
    pub probe_count: usize,
}
