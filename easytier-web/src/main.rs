#![allow(dead_code)]

#[macro_use]
extern crate rust_i18n;

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use easytier::tunnel::websocket::WSTunnelListener;
use easytier::{
    common::{
        config::{ConsoleLoggerConfig, FileLoggerConfig, LoggingConfigLoader},
        constants::EASYTIER_VERSION,
        error::Error,
        network::{local_ipv4, local_ipv6},
    },
    tunnel::{tcp::TcpTunnelListener, udp::UdpTunnelListener, TunnelListener},
    utils::{init_logger, setup_panic_handler},
};

use mimalloc::MiMalloc;
use reqwest::Client;
use serde::Serialize;

mod client_manager;
mod db;
mod migrator;
mod restful;

#[cfg(feature = "embed")]
mod web;

#[global_allocator]
static GLOBAL_MIMALLOC: MiMalloc = MiMalloc;

rust_i18n::i18n!("locales", fallback = "en");

const GATEWAY_AUTO_REGISTER_ROUTE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_REGISTER_ROUTE";
const GATEWAY_ROUTE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_ROUTE";
const GATEWAY_MANAGEMENT_URL_FILE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_MANAGEMENT_URL_FILE";

const DEFAULT_GATEWAY_ROUTE: &str = "/mt";
const DEFAULT_GATEWAY_MANAGEMENT_URL_FILE: &str = "/run/casaos/management.url";
const FALLBACK_GATEWAY_MANAGEMENT_URL_FILE: &str = "/var/run/casaos/management.url";
const GATEWAY_REGISTER_MAX_RETRIES: usize = 10;

#[derive(Serialize)]
struct GatewayRoutePayload<'a> {
    path: &'a str,
    target: &'a str,
}

#[derive(Parser, Debug)]
#[command(name = "easytier-web", author, version = EASYTIER_VERSION , about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "et.db", help = t!("cli.db").to_string())]
    db: String,

    #[arg(
        long,
        help = t!("cli.console_log_level").to_string(),
    )]
    console_log_level: Option<String>,

    #[arg(
        long,
        help = t!("cli.file_log_level").to_string(),
    )]
    file_log_level: Option<String>,

    #[arg(
        long,
        help = t!("cli.file_log_dir").to_string(),
    )]
    file_log_dir: Option<String>,

    #[arg(
        long,
        short='c',
        default_value = "22020",
        help = t!("cli.config_server_port").to_string(),
    )]
    config_server_port: u16,

    #[arg(
        long,
        short='p',
        default_value = "udp",
        help = t!("cli.config_server_protocol").to_string(),
    )]
    config_server_protocol: String,

    #[arg(
        long,
        short='a',
        default_value = "11211",
        help = t!("cli.api_server_port").to_string(),
    )]
    api_server_port: u16,

    #[arg(
        long,
        default_value = "0.0.0.0",
        help = t!("cli.api_server_addr").to_string(),
    )]
    api_server_addr: IpAddr,

    #[arg(
        long,
        help = t!("cli.geoip_db").to_string(),
    )]
    geoip_db: Option<String>,

    #[cfg(feature = "embed")]
    #[arg(
        long,
        short='l',
        help = t!("cli.web_server_port").to_string(),
    )]
    web_server_port: Option<u16>,

    #[cfg(feature = "embed")]
    #[arg(
        long,
        default_value = "0.0.0.0",
        help = t!("cli.web_server_addr").to_string(),
    )]
    web_server_addr: IpAddr,

    #[cfg(feature = "embed")]
    #[arg(
        long,
        help = t!("cli.no_web").to_string(),
        default_value = "false"
    )]
    no_web: bool,

    #[cfg(feature = "embed")]
    #[arg(
        long,
        help = t!("cli.api_host").to_string()
    )]
    api_host: Option<url::Url>,

    #[arg(
        long,
        default_value = "false",
        help = t!("cli.disable_registration").to_string(),
    )]
    disable_registration: bool,
}

impl LoggingConfigLoader for &Cli {
    fn get_console_logger_config(&self) -> ConsoleLoggerConfig {
        ConsoleLoggerConfig {
            level: self.console_log_level.clone(),
        }
    }

    fn get_file_logger_config(&self) -> FileLoggerConfig {
        FileLoggerConfig {
            dir: self.file_log_dir.clone(),
            level: self.file_log_level.clone(),
            file: None,
            size_mb: None,
            count: None,
        }
    }
}

pub fn get_listener_by_url(l: &url::Url) -> Result<Box<dyn TunnelListener>, Error> {
    Ok(match l.scheme() {
        "tcp" => Box::new(TcpTunnelListener::new(l.clone())),
        "udp" => Box::new(UdpTunnelListener::new(l.clone())),
        "ws" => Box::new(WSTunnelListener::new(l.clone())),
        _ => {
            return Err(Error::InvalidUrl(l.to_string()));
        }
    })
}

fn parse_env_bool(name: &str, default: bool) -> bool {
    let Ok(raw) = std::env::var(name) else {
        return default;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn normalize_gateway_route_path(path: &str) -> Option<String> {
    let mut route = path.trim().to_string();
    if route.is_empty() {
        return None;
    }
    if !route.starts_with('/') {
        route = format!("/{route}");
    }
    if route.len() > 1 {
        route = route.trim_end_matches('/').to_string();
    }
    if route == "/" {
        return None;
    }
    Some(route)
}

async fn resolve_gateway_management_url() -> Option<String> {
    let mut candidates: Vec<PathBuf> = vec![];
    if let Ok(custom_file) = std::env::var(GATEWAY_MANAGEMENT_URL_FILE_ENV) {
        if !custom_file.trim().is_empty() {
            candidates.push(PathBuf::from(custom_file.trim()));
        }
    }
    if candidates.is_empty() {
        candidates.push(PathBuf::from(DEFAULT_GATEWAY_MANAGEMENT_URL_FILE));
        candidates.push(PathBuf::from(FALLBACK_GATEWAY_MANAGEMENT_URL_FILE));
    }

    for candidate in candidates {
        let Ok(content) = tokio::fs::read_to_string(&candidate).await else {
            continue;
        };
        let url = content.trim().trim_end_matches('/').to_string();
        if !url.is_empty() {
            return Some(url);
        }
    }

    None
}

fn build_gateway_target(api_server_addr: IpAddr, api_server_port: u16) -> String {
    let host = match api_server_addr {
        IpAddr::V4(v4) => {
            if v4.is_unspecified() {
                "127.0.0.1".to_string()
            } else {
                v4.to_string()
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_unspecified() {
                "::1".to_string()
            } else {
                v6.to_string()
            }
        }
    };

    if host.contains(':') {
        format!("http://[{host}]:{api_server_port}")
    } else {
        format!("http://{host}:{api_server_port}")
    }
}

async fn register_route_to_gateway_if_needed(api_server_addr: IpAddr, api_server_port: u16) {
    if !parse_env_bool(GATEWAY_AUTO_REGISTER_ROUTE_ENV, true) {
        tracing::info!(
            "{}=false, skip gateway route registration",
            GATEWAY_AUTO_REGISTER_ROUTE_ENV
        );
        return;
    }

    let route = std::env::var(GATEWAY_ROUTE_ENV).unwrap_or_else(|_| DEFAULT_GATEWAY_ROUTE.into());
    let Some(route) = normalize_gateway_route_path(&route) else {
        tracing::warn!("invalid gateway route path, skip registration");
        return;
    };

    let Some(management_url) = resolve_gateway_management_url().await else {
        tracing::warn!("management url file not found, skip gateway route registration");
        return;
    };

    let endpoint = format!("{management_url}/v1/gateway/routes");
    let target = build_gateway_target(api_server_addr, api_server_port);
    let payload = GatewayRoutePayload {
        path: &route,
        target: &target,
    };

    let client = match Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!("failed to build http client for gateway route registration: {err}");
            return;
        }
    };

    for attempt in 1..=GATEWAY_REGISTER_MAX_RETRIES {
        match client.post(&endpoint).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    "registered gateway route successfully, path: {}, target: {}, attempt: {}",
                    route,
                    target,
                    attempt
                );
                return;
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::warn!(
                    "register gateway route failed, attempt: {}, status: {}, body: {}",
                    attempt,
                    status,
                    body
                );
            }
            Err(err) => {
                tracing::warn!(
                    "register gateway route failed, attempt: {}, err: {}",
                    attempt,
                    err
                );
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    tracing::warn!(
        "failed to register gateway route after {} retries, path: {}, target: {}",
        GATEWAY_REGISTER_MAX_RETRIES,
        route,
        target
    );
}

async fn get_dual_stack_listener(
    protocol: &str,
    port: u16,
) -> Result<
    (
        Option<Box<dyn TunnelListener>>,
        Option<Box<dyn TunnelListener>>,
    ),
    Error,
> {
    let is_protocol_support_dual_stack =
        protocol.trim().to_lowercase() == "tcp" || protocol.trim().to_lowercase() == "udp";
    let v6_listener = if is_protocol_support_dual_stack && local_ipv6().await.is_ok() {
        get_listener_by_url(&format!("{}://[::0]:{}", protocol, port).parse().unwrap()).ok()
    } else {
        None
    };
    let v4_listener = if local_ipv4().await.is_ok() {
        get_listener_by_url(&format!("{}://0.0.0.0:{}", protocol, port).parse().unwrap()).ok()
    } else {
        None
    };
    Ok((v6_listener, v4_listener))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let locale = sys_locale::get_locale().unwrap_or_else(|| String::from("en-US"));
    rust_i18n::set_locale(&locale);
    setup_panic_handler();

    let cli = Cli::parse();
    init_logger(&cli, false).unwrap();

    // let db = db::Db::new(":memory:").await.unwrap();
    let db = db::Db::new(cli.db).await.unwrap();
    let mut mgr = client_manager::ClientManager::new(db.clone(), cli.geoip_db);
    let (v6_listener, v4_listener) =
        get_dual_stack_listener(&cli.config_server_protocol, cli.config_server_port)
            .await
            .unwrap();
    if v4_listener.is_none() && v6_listener.is_none() {
        panic!("Listen to both IPv4 and IPv6 failed");
    }
    if let Some(listener) = v6_listener {
        mgr.add_listener(listener).await.unwrap();
    }
    if let Some(listener) = v4_listener {
        mgr.add_listener(listener).await.unwrap();
    }

    let mgr = Arc::new(mgr);

    #[cfg(feature = "embed")]
    let (web_router_restful, web_router_static) = if cli.no_web {
        (None, None)
    } else {
        let web_router = web::build_router(cli.api_host.clone());
        if cli.web_server_port.is_none()
            || (cli.web_server_port == Some(cli.api_server_port)
                && cli.web_server_addr == cli.api_server_addr)
        {
            (Some(web_router), None)
        } else {
            (None, Some(web_router))
        }
    };
    #[cfg(not(feature = "embed"))]
    let web_router_restful = None;

    let _restful_server_tasks = restful::RestfulServer::new(
        std::net::SocketAddr::new(cli.api_server_addr, cli.api_server_port),
        mgr.clone(),
        db,
        web_router_restful,
        cli.disable_registration,
    )
    .await
    .unwrap()
    .start()
    .await
    .unwrap();

    register_route_to_gateway_if_needed(cli.api_server_addr, cli.api_server_port).await;

    #[cfg(feature = "embed")]
    let _web_server_task = if let Some(web_router) = web_router_static {
        Some(
            web::WebServer::new(
                std::net::SocketAddr::new(cli.web_server_addr, cli.web_server_port.unwrap_or(0)),
                web_router,
            )
            .await
            .unwrap()
            .start()
            .await
            .unwrap(),
        )
    } else {
        None
    };

    tokio::signal::ctrl_c().await.unwrap();
}
