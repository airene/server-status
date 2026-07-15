extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use bytes::Buf;
use clap::Parser;
use http_auth_basic::Credentials;
use once_cell::sync::OnceCell;
use prost::Message;
use rust_embed::RustEmbed;
use stat_common::server_status::StatRequest;
use std::collections::HashMap;
use std::process;

mod config;
mod grpc;
mod payload;
mod stats;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{header, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;
type BodyRsp = Full<Bytes>;

static NOTFOUND: &[u8] = b"Not Found";
static UNAUTHORIZED: &[u8] = b"Unauthorized";

static G_CONFIG: OnceCell<config::Config> = OnceCell::new();
static G_STATS_MGR: OnceCell<stats::StatsMgr> = OnceCell::new();

#[derive(RustEmbed)]
#[folder = "../web"]
#[prefix = "/"]
struct Asset;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value = "config.toml")]
    config: String,
}

// stat report
async fn stats_report(req: Request<Incoming>) -> Result<Response<BodyRsp>> {
    let req_header = req.headers();
    // auth
    let mut auth_ok = false;

    if let Some(auth) = req_header.get(hyper::header::AUTHORIZATION) {
        let auth_header_value = auth.to_str()?.to_string();
        if let Ok(credentials) = Credentials::from_header(auth_header_value) {
            if let Some(cfg) = G_CONFIG.get() {
                auth_ok = cfg.auth(&credentials.user_id, &credentials.password);
            }
        }
    }
    if !auth_ok {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(UNAUTHORIZED.into())?);
    }
    // auth end

    let mut json_data: Option<serde_json::Value> = None;
    if let Ok(content_type) = req_header.get(hyper::header::CONTENT_TYPE).unwrap().clone().to_str() {
        let content_type = content_type.to_string();
        let whole_body = req.collect().await?.aggregate();
        // dbg!(content_type);
        if content_type.eq(&mime::APPLICATION_JSON.to_string()) {
            // json
            json_data = Some(serde_json::from_reader(whole_body.reader())?);
        } else if content_type.eq(&mime::APPLICATION_OCTET_STREAM.to_string()) {
            // protobuf
            let stat = StatRequest::decode(whole_body)?;
            json_data = Some(serde_json::to_value(stat)?);
        }
    }

    // report
    if let Some(mgr) = G_STATS_MGR.get() {
        mgr.report(json_data.unwrap())?;
    }

    let mut resp = HashMap::new();
    resp.insert(&"code", serde_json::Value::from(200_i32));
    let resp_str = serde_json::to_string(&resp)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::from(resp_str))?)
}

// get json data
async fn get_stats_json() -> Result<Response<BodyRsp>> {
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Headers", "*")
        .header("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
        .body(Full::from(G_STATS_MGR.get().unwrap().get_stats_json()))?)
}

async fn main_service_func(req: Request<Incoming>) -> Result<Response<BodyRsp>> {
    let req_path = req.uri().path();
    match (req.method(), req_path) {
        (&Method::POST, "/report") => stats_report(req).await,
        (&Method::GET, "/json/stats.json") => get_stats_json().await,
        (&Method::GET, "/") | (&Method::GET, "/index.html") => {
            let body = Full::from(Bytes::from(Asset::get("/index.html").unwrap().data.into_owned()));
            Ok(Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(body)?)
        }
        _ => {
            if req.method() == Method::GET
                && (req_path.starts_with("/js/")
                    || req_path.starts_with("/css/")
                    || req_path.starts_with("/img/")
                    || req_path.eq("/favicon.ico"))
            {
                if let Some(data) = Asset::get(req_path) {
                    let ct = mime_guess::from_path(req_path);
                    return Ok(Response::builder()
                        .header(header::CONTENT_TYPE, ct.first_raw().unwrap())
                        .body(Full::from(Bytes::from(data.data.into_owned())))?);
                } else {
                    error!("can't get => {:?}", req_path);
                }
            }

            // Return 404 not found response.
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(NOTFOUND.into())?)
        }
    }
}

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    let args = Args::parse();

    println!("Server start at: {} {}", env!("CARGO_BIN_NAME"), env!("APP_VERSION"));

    // config load
    if let Some(cfg) = config::from_file(&args.config) {
        debug!("config info :{}", serde_json::to_string_pretty(&cfg).unwrap());
        G_CONFIG.set(cfg).unwrap();
    } else {
        error!("can't parse config");
        process::exit(1);
    }

    // init mgr
    let mut mgr = stats::StatsMgr::new();
    mgr.init(G_CONFIG.get().unwrap())?;
    if G_STATS_MGR.set(mgr).is_err() {
        error!("can't set G_STATS_MGR");
        process::exit(1);
    }

    // serv grpc
    tokio::spawn(async move {
        let addr = &*G_CONFIG.get().unwrap().grpc_addr;
        grpc::serv_grpc(addr).await
    });

    // serv http
    let http_addr: std::net::SocketAddr = G_CONFIG.get().unwrap().http_addr.parse()?;
    eprintln!("🚀 listening on http://{}", http_addr);
    let listener = TcpListener::bind(http_addr).await?;
    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                eprintln!("shutting down");
                break;
            }
            conn = listener.accept() => {
                let (stream, _) = conn?;
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    if let Err(e) = http1::Builder::new()
                        .serve_connection(io, service_fn(main_service_func))
                        .await
                    {
                        error!("server connection error: {}", e);
                    }
                });
            }
        }
    }

    Ok(())
}
