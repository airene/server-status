//#![deny(warnings)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

fn default_grpc_addr() -> String {
    "0.0.0.0:9394".to_string()
}

fn default_http_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_workspace() -> String {
    "/opt/ServerStatus".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Host {
    pub name: String,
    pub password: String,
    pub location: String,
    #[serde(default = "u32::default")]
    pub monthstart: u32,
    #[serde(default = "bool::default")]
    pub disabled: bool,

    // user data
    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(default = "Default::default")]
    pub latest_ts: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_http_addr")]
    pub http_addr: String,
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,

    #[serde(default = "Default::default")]
    pub hosts: Vec<Host>,

    // deploy
    #[serde(default = "Default::default")]
    pub server_url: String,
    #[serde(default = "default_workspace")]
    pub workspace: String,

    #[serde(skip_deserializing)]
    pub hosts_map: HashMap<String, Host>,
}

impl Config {
    pub fn auth(&self, user: &str, pass: &str) -> bool {
        if let Some(o) = self.hosts_map.get(user) {
            return pass.eq(o.password.as_str());
        }
        false
    }
}

fn from_str(content: &str) -> Option<Config> {
    let mut o = toml::from_str::<Config>(content).unwrap();
    o.hosts_map = HashMap::new();
    // todo host pos 什么时候用 fangying
    for (idx, host) in o.hosts.iter_mut().enumerate() {
        host.pos = idx;
        if host.monthstart < 1 || host.monthstart > 31 {
            host.monthstart = 1;
        }
        o.hosts_map.insert(host.name.to_owned(), host.clone());
    }

    Some(o)
}

pub fn from_file(cfg: &str) -> Option<Config> {
    fs::read_to_string(cfg)
        .map(|contents| from_str(contents.as_str()))
        .ok()?
}
