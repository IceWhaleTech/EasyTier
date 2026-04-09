use worker::{Date, Fetch, Method, Request, SecureTransport, Socket};

use crate::{
    error::{AppError, AppResult},
    models::{CreateNodeRequest, ProbeOutcome},
};

pub async fn probe_request(request: &CreateNodeRequest) -> AppResult<ProbeOutcome> {
    probe_target(&request.host, request.port, &request.protocol).await
}

pub async fn probe_target(host: &str, port: i32, protocol: &str) -> AppResult<ProbeOutcome> {
    if !(1..=65_535).contains(&port) {
        return Err(AppError::BadRequest(
            "port must be between 1 and 65535".to_string(),
        ));
    }

    let secure_transport = match protocol.to_ascii_lowercase().as_str() {
        "ws" => return probe_http_endpoint("http", host, port).await,
        "wss" => return probe_http_endpoint("https", host, port).await,
        "tcp" => SecureTransport::Off,
        "udp" => {
            return Ok(ProbeOutcome {
                status: "unsupported".to_string(),
                is_active: false,
                response_time: None,
                error_message: Some(
                    "Cloudflare Workers does not support outbound UDP probes".to_string(),
                ),
            });
        }
        _ => SecureTransport::Off,
    };

    let mut socket = Socket::builder()
        .secure_transport(secure_transport)
        .connect(host, port as u16)
        .map_err(AppError::from)?;

    let started_at = Date::now().as_millis();
    match socket.opened().await {
        Ok(_) => {
            let _ = socket.close().await;
            Ok(ProbeOutcome {
                status: "healthy".to_string(),
                is_active: true,
                response_time: Some((Date::now().as_millis().saturating_sub(started_at)) as i32),
                error_message: None,
            })
        }
        Err(error) => {
            let _ = socket.close().await;
            Ok(ProbeOutcome {
                status: "unhealthy".to_string(),
                is_active: false,
                response_time: None,
                error_message: Some(error.to_string()),
            })
        }
    }
}

async fn probe_http_endpoint(scheme: &str, host: &str, port: i32) -> AppResult<ProbeOutcome> {
    let started_at = Date::now().as_millis();

    let mut last_error = None;
    for path in ["/connect", "/"] {
        let url = format!("{scheme}://{host}:{port}{path}");
        let request = Request::new(&url, Method::Get).map_err(AppError::from)?;
        match Fetch::Request(request).send().await {
            Ok(response) => {
                return Ok(ProbeOutcome {
                    status: "healthy".to_string(),
                    is_active: true,
                    response_time: Some(
                        (Date::now().as_millis().saturating_sub(started_at)) as i32,
                    ),
                    error_message: Some(format!("HTTP {}", response.status_code())),
                });
            }
            Err(error) => {
                last_error = Some(format!("{url}: {error}"));
            }
        }
    }

    Ok(ProbeOutcome {
        status: "unhealthy".to_string(),
        is_active: false,
        response_time: None,
        error_message: last_error,
    })
}
