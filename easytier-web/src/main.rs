#![allow(dead_code)]

#[macro_use]
extern crate rust_i18n;

use std::net::IpAddr;
use std::sync::Arc;

use clap::Parser;
use easytier::tunnel::websocket::WSTunnelListener;
use easytier::{
    common::{
        config::{ConsoleLoggerConfig, FileLoggerConfig, LoggingConfigLoader},
        constants::EASYTIER_VERSION,
        error::Error,
    },
    tunnel::{tcp::TcpTunnelListener, udp::UdpTunnelListener, TunnelListener},
    utils::{init_logger, setup_panic_handler},
};

use mimalloc::MiMalloc;

mod client_manager;
mod db;
mod gateway;
mod migrator;
mod restful;
mod system_startup;

#[cfg(feature = "embed")]
mod web;

#[global_allocator]
static GLOBAL_MIMALLOC: MiMalloc = MiMalloc;

rust_i18n::i18n!("locales", fallback = "en");

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

pub(crate) fn get_listener_by_url(l: &url::Url) -> Result<Box<dyn TunnelListener>, Error> {
    Ok(match l.scheme() {
        "tcp" => Box::new(TcpTunnelListener::new(l.clone())),
        "udp" => Box::new(UdpTunnelListener::new(l.clone())),
        "ws" => Box::new(WSTunnelListener::new(l.clone())),
        _ => {
            return Err(Error::InvalidUrl(l.to_string()));
        }
    })
}

fn get_listener_urls(protocol: &str, port: u16) -> Result<Vec<url::Url>, Error> {
    let protocol = protocol.trim().to_lowercase();

    let urls = if protocol == "tcp" || protocol == "udp" {
        vec![
            format!("{}://[::]:{}", protocol, port),
            format!("{}://0.0.0.0:{}", protocol, port),
        ]
    } else {
        vec![format!("{}://0.0.0.0:{}", protocol, port)]
    };

    urls.into_iter()
        .map(|url| url.parse().map_err(|_| Error::InvalidUrl(url.clone())))
        .collect()
}

async fn add_config_server_listeners(
    mgr: &mut client_manager::ClientManager,
    protocol: &str,
    port: u16,
) -> Result<(), String> {
    let listener_urls = get_listener_urls(protocol, port)
        .map_err(|e| format!("resolve listener urls failed: {e}"))?;
    let mut listen_errors = Vec::new();
    let mut success_count = 0;

    for listener_url in listener_urls {
        let listener = get_listener_by_url(&listener_url)
            .map_err(|e| format!("create listener for {} failed: {}", listener_url, e))?;

        match mgr.add_listener(listener).await {
            Ok(()) => {
                success_count += 1;
                tracing::info!(url = %listener_url, "config server listener started");
            }
            Err(e) => {
                tracing::warn!(url = %listener_url, error = ?e, "config server listener failed to start");
                listen_errors.push(format!("{}: {:?}", listener_url, e));
            }
        }
    }

    if success_count == 0 {
        return Err(format!(
            "all config server listeners failed to start: {}",
            listen_errors.join("; ")
        ));
    }

    if !listen_errors.is_empty() {
        tracing::warn!(errors = ?listen_errors, "config server started with partial listeners");
    }

    Ok(())
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
    add_config_server_listeners(
        &mut mgr,
        &cli.config_server_protocol,
        cli.config_server_port,
    )
    .await
    .unwrap_or_else(|e| panic!("failed to start config server listeners: {e}"));

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

    let _system_startup_task = system_startup::start(mgr.clone());

    gateway::register_route_to_gateway_if_needed(cli.api_server_addr, cli.api_server_port).await;

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

#[cfg(test)]
mod tests {
    use super::get_listener_urls;

    #[test]
    fn config_server_listener_urls_use_dual_stack_for_tcp_and_udp() {
        let tcp_urls = get_listener_urls("tcp", 22020).unwrap();
        assert_eq!(tcp_urls.len(), 2);
        assert_eq!(tcp_urls[0].scheme(), "tcp");
        assert_eq!(tcp_urls[0].host_str(), Some("[::]"));
        assert_eq!(tcp_urls[0].port(), Some(22020));
        assert_eq!(tcp_urls[1].scheme(), "tcp");
        assert_eq!(tcp_urls[1].host_str(), Some("0.0.0.0"));
        assert_eq!(tcp_urls[1].port(), Some(22020));

        let udp_urls = get_listener_urls("udp", 22020).unwrap();
        assert_eq!(udp_urls.len(), 2);
        assert_eq!(udp_urls[0].scheme(), "udp");
        assert_eq!(udp_urls[0].host_str(), Some("[::]"));
        assert_eq!(udp_urls[0].port(), Some(22020));
        assert_eq!(udp_urls[1].scheme(), "udp");
        assert_eq!(udp_urls[1].host_str(), Some("0.0.0.0"));
        assert_eq!(udp_urls[1].port(), Some(22020));
    }

    #[test]
    fn config_server_listener_urls_keep_single_stack_for_ws() {
        let ws_urls = get_listener_urls("ws", 22020).unwrap();
        assert_eq!(ws_urls.len(), 1);
        assert_eq!(ws_urls[0].scheme(), "ws");
        assert_eq!(ws_urls[0].host_str(), Some("0.0.0.0"));
        assert_eq!(ws_urls[0].port(), Some(22020));
    }
}
