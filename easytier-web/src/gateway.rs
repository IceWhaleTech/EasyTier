use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Serialize;

const GATEWAY_AUTO_REGISTER_ROUTE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_REGISTER_ROUTE";
const GATEWAY_ROUTE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_ROUTE";
const GATEWAY_AUTO_REGISTER_API_ROUTE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_REGISTER_API_ROUTE";
const GATEWAY_API_ROUTE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_API_ROUTE";
const GATEWAY_MANAGEMENT_URL_FILE_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_MANAGEMENT_URL_FILE";
const GATEWAY_REGISTER_WAIT_SECS_ENV: &str = "ZIMAOS_EASYTIER_WEB_GATEWAY_REGISTER_WAIT_SECS";

const DEFAULT_GATEWAY_ROUTE: &str = "/mt";
const DEFAULT_GATEWAY_API_ROUTE: &str = "/api/v1";
const DEFAULT_GATEWAY_MANAGEMENT_URL_FILE: &str = "/run/casaos/management.url";
const FALLBACK_GATEWAY_MANAGEMENT_URL_FILE: &str = "/var/run/casaos/management.url";
const DEFAULT_GATEWAY_REGISTER_WAIT_SECS: u64 = 300;
const GATEWAY_REGISTER_MAX_RETRIES: usize = 10;

#[derive(Serialize)]
struct GatewayRoutePayload<'a> {
    path: &'a str,
    target: &'a str,
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

fn parse_env_u64(name: &str, default: u64) -> u64 {
    let Ok(raw) = std::env::var(name) else {
        return default;
    };
    raw.trim().parse::<u64>().unwrap_or(default)
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

async fn wait_for_gateway_management_url() -> Option<String> {
    let wait_secs = parse_env_u64(
        GATEWAY_REGISTER_WAIT_SECS_ENV,
        DEFAULT_GATEWAY_REGISTER_WAIT_SECS,
    );
    if wait_secs == 0 {
        return resolve_gateway_management_url().await;
    }

    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    loop {
        if let Some(url) = resolve_gateway_management_url().await {
            return Some(url);
        }

        if Instant::now() >= deadline {
            return None;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
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

async fn register_single_gateway_route(
    client: &Client,
    endpoint: &str,
    path: &str,
    target: &str,
) -> bool {
    let payload = GatewayRoutePayload { path, target };
    for attempt in 1..=GATEWAY_REGISTER_MAX_RETRIES {
        match client.post(endpoint).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(
                    "registered gateway route successfully, path: {}, target: {}, attempt: {}",
                    path,
                    target,
                    attempt
                );
                return true;
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                tracing::warn!(
                    "register gateway route failed, path: {}, attempt: {}, status: {}, body: {}",
                    path,
                    attempt,
                    status,
                    body
                );
            }
            Err(err) => {
                tracing::warn!(
                    "register gateway route failed, path: {}, attempt: {}, err: {}",
                    path,
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
        path,
        target
    );
    false
}

pub async fn register_route_to_gateway_if_needed(api_server_addr: IpAddr, api_server_port: u16) {
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

    let Some(management_url) = wait_for_gateway_management_url().await else {
        tracing::warn!(
            "management url file not found after waiting {}s, skip gateway route registration",
            parse_env_u64(
                GATEWAY_REGISTER_WAIT_SECS_ENV,
                DEFAULT_GATEWAY_REGISTER_WAIT_SECS
            )
        );
        return;
    };

    let endpoint = format!("{management_url}/v1/gateway/routes");
    let target = build_gateway_target(api_server_addr, api_server_port);

    let client = match Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!("failed to build http client for gateway route registration: {err}");
            return;
        }
    };

    let mut routes = vec![route];
    if parse_env_bool(GATEWAY_AUTO_REGISTER_API_ROUTE_ENV, true) {
        let api_route = std::env::var(GATEWAY_API_ROUTE_ENV)
            .unwrap_or_else(|_| DEFAULT_GATEWAY_API_ROUTE.into());
        if let Some(api_route) = normalize_gateway_route_path(&api_route) {
            if !routes.iter().any(|r| r == &api_route) {
                routes.push(api_route);
            }
        } else {
            tracing::warn!(
                "invalid gateway api route path from {}, skip api route registration",
                GATEWAY_API_ROUTE_ENV
            );
        }
    } else {
        tracing::info!(
            "{}=false, skip gateway api route registration",
            GATEWAY_AUTO_REGISTER_API_ROUTE_ENV
        );
    }

    for path in routes {
        register_single_gateway_route(&client, &endpoint, &path, &target).await;
    }
}
