#![deny(warnings)]
extern crate pretty_env_logger;
#[macro_use] extern crate log;
use clap::Parser;
use hyper::header;
use prost::Message;
use std::net::ToSocketAddrs;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{System, SystemExt};

use stat_common::server_status::StatRequest;
type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;
mod grpc;
mod status;

const INTERVAL_MS: u64 = 1000;

#[derive(Parser, Debug, Clone)]
#[clap(author, version = env!("APP_VERSION"), about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_parser, default_value = "http://127.0.0.1:8080/report")]
    addr: String,
    #[clap(short, long, value_parser, default_value = "h1", help = "username")]
    user: String,
    #[clap(short, long, value_parser, default_value = "p1", help = "password")]
    pass: String,
    #[clap(short = 'n', long, value_parser, help = "enable vnstat, default:false")]
    vnstat: bool,
    #[clap(long = "disable-tupd", value_parser, help = "disable t/u/p/d, default:false")]
    disable_tupd: bool,
    #[clap(long = "disable-ping", value_parser, help = "disable ping, default:false")]
    disable_ping: bool,

    #[clap(long = "ip-info", value_parser, help = "show ip info, default:false")]
    ip_info: bool,
    #[clap(long = "json", value_parser, help = "use json protocol, default:false")]
    json: bool,

    #[clap(long = "alias", value_parser, default_value = "unknown", help = "alias for host")]
    alias: String,
    #[clap(short, long, value_parser, default_value = "0", help = "weight for rank")]
    weight: u64,

    #[clap(short = 't', long = "type", value_parser, default_value = "", help = "host type")]
    host_type: String,
    #[clap(long, value_parser, default_value = "", help = "location")]
    location: String,
    // #[clap(long = "debug", help = "debug mode, default:false")]
    // debug: bool,
}

fn sample_all(args: &Args, stat_base: &StatRequest) -> StatRequest {
    // dbg!(&stat_base);
    let mut stat_rt = stat_base.clone();

    #[cfg(all(feature = "native", not(feature = "sysinfo")))]
    status::sample(args, &mut stat_rt);

    stat_rt.latest_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    stat_rt
}

fn http_report(args: &Args, stat_base: &mut StatRequest) -> Result<()> {
    let mut domain = args.addr.split('/').collect::<Vec<&str>>()[2].to_owned();
    if !domain.contains(':') {
        if args.addr.contains("https") {
            domain = format!("{}:443", domain);
        } else {
            domain = format!("{}:80", domain);
        }
    }
    let tcp_addr = domain.to_socket_addrs()?.next().unwrap();
    let ipv4 = tcp_addr.is_ipv4();
    if ipv4 {
        stat_base.online4 = ipv4;
    }


    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(format!("{}/{}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION")))
        .build()?;
    loop {
        let stat_rt = sample_all(args, stat_base);
        dbg!(&stat_rt);
        let body_data: Option<Vec<u8>>;
        let mut content_type = "application/octet-stream";
        if args.json {
            let data = serde_json::to_string(&stat_rt)?;
            trace!("json_str => {:?}", serde_json::to_string(&data)?);
            body_data = Some(data.into());
            content_type = "application/json";
        } else {
            let buf = stat_rt.encode_to_vec();
            body_data = Some(buf);
            // content_type = "application/octet-stream";
        }
        // byte 581, json str 1281
        // dbg!(&body_data.as_ref().unwrap().len());

        let client = http_client.clone();
        let url = args.addr.to_string();
        let auth_pass = args.pass.to_string();
        let auth_user: String;
        let ssr_auth: &str;
        auth_user = args.user.to_string();
        ssr_auth = "single";

        // http
        tokio::spawn(async move {
            match client
                .post(&url)
                .basic_auth(auth_user, Some(auth_pass))
                .timeout(Duration::from_secs(3))
                .header(header::CONTENT_TYPE, content_type)
                .header("ssr-auth", ssr_auth)
                .body(body_data.unwrap())
                .send()
                .await
            {
                Ok(resp) => {
                    info!("report resp => {:?}", resp);
                }
                Err(err) => {
                    error!("report error => {:?}", err);
                }
            }
        });

        thread::sleep(Duration::from_millis(INTERVAL_MS));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    let args = Args::parse();
    dbg!(&args);

    // support check
    if !System::IS_SUPPORTED {
        panic!("当前系统不支持，请切换到Python跨平台版本!");
    }

    // use native
    #[cfg(all(feature = "native", not(feature = "sysinfo")))]
    {
        eprintln!("enable feature native");
        status::start_cpu_percent_collect_t();
        status::start_net_speed_collect_t();
    }

    let (ipv4, ipv6) = status::get_network();
    eprintln!("get_network (ipv4, ipv6) => ({}, {})", ipv4, ipv6);

    let mut stat_base = StatRequest {
        name: args.user.to_string(),
        frame: "data".to_string(),
        online4: ipv4,
        vnstat: args.vnstat,
        weight: args.weight,
        version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };

    if !args.host_type.is_empty() {
        stat_base.r#type = args.host_type.to_owned();
    }
    if !args.location.is_empty() {
        stat_base.location = args.location.to_owned();
    }
    dbg!(&stat_base);

    if args.addr.starts_with("http") {
        let result = http_report(&args, &mut stat_base);
        dbg!(&result);
    } else if args.addr.starts_with("grpc") {
        let result = grpc::report(&args, &mut stat_base).await;
        dbg!(&result);
    } else {
        eprint!("invalid addr scheme!");
    }

    Ok(())
}
