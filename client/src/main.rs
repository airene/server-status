//#![deny(warnings)]
//下面三行是一起的 see https://crates.io/crates/pretty_env_logger
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use clap::Parser;
use prost::Message;
use reqwest::header;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use stat_common::server_status::StatRequest;

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;

mod grpc;
mod status;

#[derive(Parser, Debug, Clone)]
#[clap(author, version = env ! ("APP_VERSION"), about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_parser, default_value = "http://127.0.0.1:8080/report")]
    addr: String,
    #[clap(short, long, value_parser, default_value = "h1", help = "username")]
    user: String,
    #[clap(short, long, value_parser, default_value = "p1", help = "password")]
    pass: String,
    #[clap(
        short = 'i',
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        default_value_t = 60,
        help = "report interval in seconds"
    )]
    interval: u64,
    #[clap(short = 'n', long, value_parser, help = "enable vnstat, default:false")]
    vnstat: bool,
    #[clap(long = "json", value_parser, help = "use json protocol, default:false")]
    json: bool,
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
    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .connect_timeout(Duration::from_secs(5))
        .user_agent(format!("{}/{}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION")))
        .build()?;
    loop {
        let stat_rt = sample_all(args, stat_base);
        // dbg!(&stat_rt);
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
        }
        // byte 581, json str 1281
        // dbg!(&body_data.as_ref().unwrap().len());

        let client = http_client.clone();
        let url = args.addr.to_string();
        let auth_pass = args.pass.to_string();
        let auth_user: String = args.user.to_string();
        // http
        tokio::spawn(async move {
            match client
                .post(&url)
                .basic_auth(auth_user, Some(auth_pass))
                .timeout(Duration::from_secs(3))
                .header(header::CONTENT_TYPE, content_type)
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

        thread::sleep(Duration::from_secs(args.interval));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    let args = Args::parse();
    // dbg!(&args);

    // support check
    if !sysinfo::IS_SUPPORTED_SYSTEM {
        panic!("当前系统不支持!");
    }

    // use native 计算cpu使用率和网速
    #[cfg(all(feature = "native", not(feature = "sysinfo")))]
    {
        eprintln!("enable feature native");
        status::start_cpu_percent_collect_t();
        status::start_net_speed_collect_t();
    }

    let mut stat_base = StatRequest {
        name: args.user.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };

    // dbg!(&stat_base);

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

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::{CommandFactory, Parser};

    #[test]
    fn report_interval_defaults_to_sixty_seconds() {
        let args = Args::try_parse_from(["stat_client"]).unwrap();
        assert_eq!(args.interval, 60);
    }

    #[test]
    fn short_report_interval_is_parsed_in_seconds() {
        let args = Args::try_parse_from(["stat_client", "-i", "10"]).unwrap();
        assert_eq!(args.interval, 10);
    }

    #[test]
    fn long_report_interval_is_parsed_in_seconds() {
        let args = Args::try_parse_from(["stat_client", "--interval", "10"]).unwrap();
        assert_eq!(args.interval, 10);
    }

    #[test]
    fn zero_report_interval_is_rejected() {
        assert!(Args::try_parse_from(["stat_client", "--interval", "0"]).is_err());
    }

    #[test]
    fn help_describes_report_interval_in_seconds() {
        let help = Args::command().render_help().to_string();
        assert!(help.contains("report interval in seconds"));
    }
}
