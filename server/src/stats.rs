#![allow(unused)]
use anyhow::Result;
use chrono::{Datelike, Local, Timelike};
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::borrow::Borrow;
use std::borrow::BorrowMut;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc::sync_channel;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Host;
use crate::payload::{HostStat, StatsResp};

const SAVE_INTERVAL: u64 = 60;

static STAT_SENDER: OnceCell<SyncSender<Cow<HostStat>>> = OnceCell::new();

pub struct StatsMgr {
    resp_json: Arc<Mutex<String>>,
    stats_data: Arc<Mutex<StatsResp>>,
}

impl StatsMgr {
    pub fn new() -> Self {
        Self {
            resp_json: Arc::new(Mutex::new("{}".to_string())),
            stats_data: Arc::new(Mutex::new(StatsResp::new())),
        }
    }

    fn load_last_network(&mut self, hosts_map: &mut HashMap<String, Host>) {
        let contents = fs::read_to_string("stats.json").unwrap_or_default();
        if contents.is_empty() {
            return;
        }

        if let Ok(stats_json) = serde_json::from_str::<serde_json::Value>(contents.as_str()) {
            if let Some(servers) = stats_json["servers"].as_array() {
                for v in servers {
                    if let (Some(name), Some(last_network_in), Some(last_network_out)) = (
                        v["name"].as_str(),
                        v["last_network_in"].as_u64(),
                        v["last_network_out"].as_u64(),
                    ) {
                        if let Some(srv) = hosts_map.get_mut(name) {
                            srv.last_network_in = last_network_in;
                            srv.last_network_out = last_network_out;

                            trace!("{} => last in/out ({}/{}))", &name, last_network_in, last_network_out);
                        }
                    } else {
                        error!("invalid json => {:?}", v);
                    }
                }
                trace!("load stats.json succ!");
            }
        } else {
            warn!("ignore invalid stats.json");
        }
    }

    pub fn init(
        &mut self,
        cfg: &'static crate::config::Config
    ) -> Result<()> {
        let hosts_map_base = Arc::new(Mutex::new(cfg.hosts_map.clone()));

        // load last_network_in/out
        if let Ok(mut hosts_map) = hosts_map_base.lock() {
            self.load_last_network(&mut *hosts_map);
        }

        let (stat_tx, stat_rx) = sync_channel(512);
        STAT_SENDER.set(stat_tx).unwrap();

        let stat_map: Arc<Mutex<HashMap<String, Cow<HostStat>>>> = Arc::new(Mutex::new(HashMap::new()));

        // stat_rx thread
        let hosts_map_1 = hosts_map_base.clone();
        let stat_map_1 = stat_map.clone();

        thread::spawn(move || loop {
            while let Ok(stat) = stat_rx.recv() {
                trace!("recv stat `{:?}", stat);

                let mut stat_c = stat;
                let mut stat_t = stat_c.to_mut();
                //
                if let Ok(mut hosts_map) = hosts_map_1.lock() {
                    let host_info = hosts_map.get_mut(&stat_t.name);
                    if host_info.is_none() {
                        error!("invalid stat `{:?}", stat_t);
                        continue;
                    }
                    let info = host_info.unwrap();
                    if info.disabled {
                        continue;
                    }
                    // 补齐
                    if stat_t.location.is_empty() {
                        stat_t.location = info.location.to_string();
                    }

                    info.latest_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                    stat_t.latest_ts = info.latest_ts;

                    // last_network_in/out
                    if !stat_t.vnstat {
                        let local_now = Local::now();
                        if info.last_network_in == 0
                            || (stat_t.network_in != 0 && info.last_network_in > stat_t.network_in)
                            || (local_now.day() == info.monthstart && local_now.hour() == 0 && local_now.minute() < 5)
                        {
                            info.last_network_in = stat_t.network_in;
                            info.last_network_out = stat_t.network_out;
                        } else {
                            stat_t.last_network_in = info.last_network_in;
                            stat_t.last_network_out = info.last_network_out;
                        }
                    }

                    // uptime str
                    let day = (stat_t.uptime as f64 / 3600.0 / 24.0) as i64;
                    if day > 0 {
                        stat_t.uptime_str = format!("{} 天", day);
                    } else {
                        stat_t.uptime_str = format!(
                            "{:02}:{:02}:{:02}",
                            (stat_t.uptime as f64 / 3600.0) as i64,
                            (stat_t.uptime as f64 / 60.0) as i64 % 60,
                            stat_t.uptime % 60
                        );
                    }

                    info!("update stat `{:?}", stat_t);
                    if let Ok(mut host_stat_map) = stat_map_1.lock() {
                        host_stat_map.insert(stat_c.name.to_string(), stat_c);
                        //trace!("{:?}", host_stat_map);
                    }
                }
            }
        });

        // timer thread
        let resp_json = self.resp_json.clone();
        let stats_data = self.stats_data.clone();
        let hosts_map_2 = hosts_map_base.clone();
        let stat_map_2 = stat_map.clone();
        let mut latest_save_ts = 0_u64;
        let mut latest_group_gc = 0_u64;
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(500));

            let mut resp = StatsResp::new();
            let now = resp.updated;

            if let Ok(mut host_stat_map) = stat_map_2.lock() {
                for (_, stat) in host_stat_map.iter_mut() {
                    if stat.disabled {
                        resp.servers.push(stat.to_owned().into_owned());
                        continue;
                    }
                    let stat_c = stat.borrow_mut();
                    let o = stat_c.to_mut();
                    // 30s 下线
                    if o.latest_ts + cfg.offline_threshold < now {
                        o.online4 = false;
                    }

                    resp.servers.push(stat_c.to_owned().into_owned());
                }
            }

            // last_network_in/out save /60s
            if latest_save_ts + SAVE_INTERVAL < now {
                latest_save_ts = now;
                if !resp.servers.is_empty() {
                    if let Ok(mut file) = File::create("stats.json") {
                        file.write(serde_json::to_string(&resp).unwrap().as_bytes());
                        file.flush();
                        trace!("save stats.json succ!");
                    } else {
                        error!("save stats.json fail!");
                    }
                }
            }
            //
            if let Ok(mut o) = resp_json.lock() {
                *o = serde_json::to_string(&resp).unwrap();
            }
            if let Ok(mut o) = stats_data.lock() {
                *o = resp;
            }
        });

        Ok(())
    }

    pub fn get_stats(&self) -> Arc<Mutex<StatsResp>> {
        self.stats_data.clone()
    }

    pub fn get_stats_json(&self) -> String {
        self.resp_json.lock().unwrap().to_string()
    }

    pub fn report(&self, data: serde_json::Value) -> Result<()> {
        lazy_static! {
            static ref SENDER: SyncSender<Cow<'static, HostStat>> = STAT_SENDER.get().unwrap().clone();
        }

        match serde_json::from_value(data) {
            Ok(stat) => {
                trace!("send stat => {:?} ", stat);
                SENDER.send(Cow::Owned(stat));
            }
            Err(err) => {
                error!("report error => {:?}", err);
            }
        };
        Ok(())
    }
}
