#![deny(warnings)]
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

fn default_as_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostStat {
    pub name: String,
    #[serde(rename = "type", default = "Default::default")]
    pub host_type: String,
    #[serde(default = "Default::default")]
    pub location: String,

    #[serde(default = "bool::default")]
    pub vnstat: bool,

    #[serde(default = "default_as_true")]
    pub online4: bool,

    #[serde(rename(deserialize = "uptime"), skip_serializing)]
    pub uptime: u64,
    #[serde(rename(serialize = "uptime"), skip_deserializing)]
    pub uptime_str: String,

    pub network_rx: u64,
    pub network_tx: u64,
    pub network_in: u64,
    pub network_out: u64,

    #[serde(default)]
    pub last_network_in: u64,
    #[serde(default)]
    pub last_network_out: u64,

    pub cpu: f32,
    pub memory_total: u64,
    pub memory_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub hdd_total: u64,
    pub hdd_used: u64,

    #[serde(skip_deserializing)]
    pub custom: String,

    // user data
    #[serde(skip_deserializing)]
    pub latest_ts: u64,

    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(skip_serializing, skip_deserializing)]
    pub disabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatsResp {
    pub updated: u64,
    pub servers: Vec<HostStat>,
}
impl StatsResp {
    pub fn new() -> Self {
        Self {
            updated: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            servers: Vec::new(),
        }
    }
}
