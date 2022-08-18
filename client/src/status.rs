#![allow(unused)]
use chrono::{Datelike, Local};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::process::Command;
use std::str;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::Args;
use stat_common::server_status::StatRequest;

const SAMPLE_PERIOD: u64 = 1000; //ms

pub fn get_uptime() -> u64 {
    fs::read_to_string("/proc/uptime")
        .map(|contents| {
            if let Some(s) = contents.split('.').next() {
                return s.parse::<u64>().unwrap_or(0);
            }
            0
        })
        .unwrap()
}

static MEMORY_REGEX: &str = r#"^(?P<key>\S*):\s*(?P<value>\d*)\s*kB"#;
lazy_static! {
    static ref MEMORY_REGEX_RE: Regex = Regex::new(MEMORY_REGEX).unwrap();
}
pub fn get_memory() -> (u64, u64) {
    let file = File::open("/proc/meminfo").unwrap();
    let buf_reader = BufReader::new(file);
    let mut res_dict = HashMap::new();
    for line in buf_reader.lines() {
        let l = line.unwrap();
        if let Some(caps) = MEMORY_REGEX_RE.captures(&l) {
            res_dict.insert(caps["key"].to_string(), caps["value"].parse::<u64>().unwrap());
        };
    }

    let mem_total = res_dict["MemTotal"];

    let mem_used =
        mem_total - res_dict["MemFree"] - res_dict["Buffers"] - res_dict["Cached"] - res_dict["SReclaimable"];

    (mem_total, mem_used)
}

static IFACE_IGNORE_VEC: &[&str] = &["lo", "docker", "vnet", "veth", "vmbr", "kube", "br-"];

pub fn get_vnstat_traffic() -> (u64, u64) {
    let local_now = Local::now();
    let (mut network_in, mut network_out) = (0, 0);
    let a = Command::new("/usr/bin/vnstat")
        .args(&["--json", "m"])
        .output()
        .expect("failed to execute vnstat")
        .stdout;
    let b = str::from_utf8(&a).unwrap();
    let j: HashMap<&str, serde_json::Value> = serde_json::from_str(b).unwrap();
    for iface in j["interfaces"].as_array().unwrap() {
        let name = iface["name"].as_str().unwrap();
        if IFACE_IGNORE_VEC.iter().any(|sk| name.contains(*sk)) {
            continue;
        }
        let total_o = iface["traffic"]["total"].as_object().unwrap();
        network_in += total_o["rx"].as_u64().unwrap();
        network_out += total_o["tx"].as_u64().unwrap();
    }

    (network_in, network_out)
}

static TRAFFIC_REGEX: &str =
    r#"([^\s]+):[\s]{0,}(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)"#;
lazy_static! {
    static ref TRAFFIC_REGEX_RE: Regex = Regex::new(TRAFFIC_REGEX).unwrap();
}
pub fn get_sys_traffic() -> (u64, u64) {
    let (mut network_in, mut network_out) = (0, 0);
    let file = File::open("/proc/net/dev").unwrap();
    let buf_reader = BufReader::new(file);
    for line in buf_reader.lines() {
        let l = line.unwrap();

        TRAFFIC_REGEX_RE.captures(&l).and_then(|caps| {
            // println!("caps[0]=>{:?}", caps.get(0).unwrap().as_str());
            let name = caps.get(1).unwrap().as_str();
            if IFACE_IGNORE_VEC.iter().any(|sk| name.contains(*sk)) {
                return None;
            }
            let net_in = caps.get(2).unwrap().as_str().parse::<u64>().unwrap();
            let net_out = caps.get(10).unwrap().as_str().parse::<u64>().unwrap();

            network_in += net_in;
            network_out += net_out;
            Some(())
        });
    }

    (network_in, network_out)
}

static DF_CMD: &str = "df -Tlm --total -t ext4 -t ext3 -t ext2 -t reiserfs -t jfs -t ntfs -t fat32 -t btrfs -t fuseblk -t zfs -t simfs -t xfs";

pub fn get_hdd() -> (u64, u64) {
    let (mut hdd_total, mut hdd_used) = (0, 0);
    let a = &Command::new("/bin/sh")
        .args(&["-c", DF_CMD])
        .output()
        .expect("failed to execute df")
        .stdout;
    let _ = str::from_utf8(a).map(|s| {
        s.trim().split('\n').last().map(|s| {
            let vec: Vec<&str> = s.split_whitespace().collect();
            // dbg!(&vec);
            hdd_total = vec[2].parse::<u64>().unwrap();
            hdd_used = vec[3].parse::<u64>().unwrap();
            Some(())
        });
    });

    (hdd_total, hdd_used)
}

#[derive(Debug, Default)]
pub struct NetSpeed {
    pub diff: f64,
    pub clock: f64,
    pub netrx: u64,
    pub nettx: u64,
    pub avgrx: u64,
    pub avgtx: u64,
}

lazy_static! {
    pub static ref G_NET_SPEED: Arc<Mutex<NetSpeed>> = Arc::new(Default::default());
}

pub fn start_net_speed_collect_t() {
    thread::spawn(|| loop {
        let _ = File::open("/proc/net/dev").map(|file| {
            let buf_reader = BufReader::new(file);
            let (mut avgrx, mut avgtx) = (0, 0);
            for line in buf_reader.lines() {
                let l = line.unwrap();
                let v: Vec<&str> = l.split(':').collect();
                if v.len() < 2 {
                    continue;
                }

                if IFACE_IGNORE_VEC.iter().any(|sk| v[0].contains(*sk)) {
                    continue;
                }
                let v1: Vec<&str> = v[1].split_whitespace().collect();
                avgrx += v1[0].parse::<u64>().unwrap();
                avgtx += v1[8].parse::<u64>().unwrap();
            }

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as f64;

            if let Ok(mut t) = G_NET_SPEED.lock() {
                t.diff = now - t.clock;
                t.clock = now;
                t.netrx = ((avgrx - t.avgrx) as f64 / t.diff) as u64;
                t.nettx = ((avgtx - t.avgtx) as f64 / t.diff) as u64;
                t.avgrx = avgrx;
                t.avgtx = avgtx;

                // dbg!(&t);
            }
        });
        thread::sleep(Duration::from_millis(SAMPLE_PERIOD));
    });
}

lazy_static! {
    pub static ref G_CPU_PERCENT: Arc<Mutex<f64>> = Arc::new(Default::default());
}
pub fn start_cpu_percent_collect_t() {
    let mut pre_cpu: Vec<u64> = vec![0, 0, 0, 0];
    thread::spawn(move || loop {
        let _ = File::open("/proc/stat").map(|file| {
            let mut buf_reader = BufReader::new(file);
            let mut buf = String::new();
            let _ = buf_reader.read_line(&mut buf).map(|_| {
                let cur_cpu = buf
                    .split_whitespace()
                    .enumerate()
                    .filter(|&(idx, _)| idx > 0 && idx < 5)
                    .map(|(_, e)| e.parse::<u64>().unwrap())
                    .collect::<Vec<_>>();

                let pre: u64 = pre_cpu.iter().sum();
                let cur: u64 = cur_cpu.iter().sum();
                let mut st = cur - pre;
                if st == 0 {
                    st = 1;
                }

                let res = 100.0 - (100.0 * (cur_cpu[3] - pre_cpu[3]) as f64 / st as f64);

                // dbg!(&pre_cpu);
                // dbg!(&cur_cpu);

                pre_cpu = cur_cpu;

                if let Ok(mut cpu_percent) = G_CPU_PERCENT.lock() {
                    *cpu_percent = res.round();
                    // dbg!(cpu_percent);
                }
            });
        });

        thread::sleep(Duration::from_millis(SAMPLE_PERIOD));
    });
}

pub fn sample(args: &Args, stat: &mut StatRequest) {
    stat.uptime = get_uptime();

    let (mem_total, mem_used) = get_memory();
    stat.memory_total = mem_total;
    stat.memory_used = mem_used;

    let (hdd_total, hdd_used) = get_hdd();
    stat.hdd_total = hdd_total;
    stat.hdd_used = hdd_used;

    if args.vnstat {
        let (network_in, network_out) = get_vnstat_traffic();
        stat.network_in = network_in;
        stat.network_out = network_out;
    } else {
        let (network_in, network_out) = get_sys_traffic();
        stat.network_in = network_in;
        stat.network_out = network_out;
    }

    if let Ok(o) = G_CPU_PERCENT.lock() {
        stat.cpu = *o;
    }

    if let Ok(o) = G_NET_SPEED.lock() {
        stat.network_rx = o.netrx;
        stat.network_tx = o.nettx;
    }
}
