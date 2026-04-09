use std::time::Instant;

use worker::{SecureTransport, Socket};

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
        "tcp" | "ws" => SecureTransport::Off,
        "wss" => SecureTransport::On,
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

    let started_at = Instant::now();
    match socket.opened().await {
        Ok(_) => {
            let _ = socket.close().await;
            Ok(ProbeOutcome {
                status: "healthy".to_string(),
                is_active: true,
                response_time: Some(started_at.elapsed().as_millis() as i32),
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
