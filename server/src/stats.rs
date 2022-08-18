#![allow(unused)]

use anyhow::Result;
use chrono::{Datelike, Local, Timelike};
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::borrow::BorrowMut;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc;
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
    resp_json: Arc<Mutex<String>>
}

impl StatsMgr {
    pub fn new() -> Self {
        Self {
            resp_json: Arc::new(Mutex::new("{}".to_string()))
        }
    }

    pub fn init(&mut self, cfg: &'static crate::config::Config) -> Result<()> {
        let hosts_map_base = Arc::new(Mutex::new(cfg.hosts_map.clone()));

        let (stat_tx, stat_rx) = mpsc::sync_channel(512);
        STAT_SENDER.set(stat_tx).unwrap();

        let stat_map: Arc<Mutex<HashMap<String, Cow<HostStat>>>> = Arc::new(Mutex::new(HashMap::new()));

        // stat_rx thread
        //let hosts_map_1 = hosts_map_base.clone();
        let stat_map_1 = stat_map.clone();

        thread::spawn(move || loop {
            while let Ok(stat) = stat_rx.recv() {
                trace!("recv stat `{:?}", stat);
                // 好像是解包
                let mut stat_c = stat;
                let mut stat_t = stat_c.to_mut();
                //
                if let Ok(mut hosts_map) = hosts_map_base.lock() {
                    let host_info = hosts_map.get_mut(&stat_t.name);
                    if host_info.is_none() {
                        error!("invalid stat `{:?}", stat_t);
                        continue;
                    }
                    let info = host_info.unwrap();
                    if info.disabled {
                        continue;
                    }

                    stat_t.location = info.location.to_string();
                    // 直接当前时间赋值 host中也有这个字段 看要不要用 fangying
                    // info.latest_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                    // stat_t.latest_ts = info.latest_ts;
                    stat_t.latest_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

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

        // timer thread 做需要返回数据的服务 返回用到的数据的更新频率
        let resp_json = self.resp_json.clone();
        let stat_map_2 = stat_map.clone();
        let mut latest_save_ts = 0_u64;
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(1000));

            let mut resp = StatsResp::new();
            let now = resp.updated;

            if let Ok(mut host_stat_map) = stat_map_2.lock() {
                // 如果禁用了
                for (_, stat) in host_stat_map.iter_mut() {
                    if stat.disabled {
                        resp.servers.push(stat.to_owned().into_owned());
                        continue;
                    }
                    let stat_c = stat.borrow_mut();
                    resp.servers.push(stat_c.to_owned().into_owned());
                }
            }

            //
            if let Ok(mut o) = resp_json.lock() {
                *o = serde_json::to_string(&resp).unwrap();
            }
        });

        Ok(())
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
