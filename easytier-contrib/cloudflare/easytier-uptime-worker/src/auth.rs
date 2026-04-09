use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    config::AppConfig,
    error::{AppError, AppResult},
    models::AdminLoginResponse,
};

const KV_BINDING: &str = "ADMIN_SESSIONS";

pub async fn login(
    env: &worker::Env,
    config: &AppConfig,
    password: &str,
) -> AppResult<AdminLoginResponse> {
    if password != config.admin_password {
        return Err(AppError::Unauthorized("invalid password".to_string()));
    }

    let token = Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::seconds(config.token_ttl_seconds as i64))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    env.kv(KV_BINDING)?
        .put(&session_key(&token), "admin")
        .map_err(|error| AppError::Internal(error.to_string()))?
        .expiration_ttl(config.token_ttl_seconds)
        .execute()
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    Ok(AdminLoginResponse { token, expires_at })
}

pub async fn require_admin(env: &worker::Env, headers: &HeaderMap) -> AppResult<()> {
    let token = extract_bearer_token(headers)?;
    let session = env
        .kv(KV_BINDING)?
        .get(&session_key(token))
        .text()
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    if session.is_none() {
        return Err(AppError::Unauthorized("invalid token".to_string()));
    }

    Ok(())
}
fn extract_bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or_else(|| AppError::Unauthorized("missing authorization header".to_string()))?;
    let value = value
        .to_str()
        .map_err(|_| AppError::Unauthorized("invalid authorization header".to_string()))?;
    value
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("invalid authorization format".to_string()))
}

fn session_key(token: &str) -> String {
    format!("admin-session:{token}")
}
