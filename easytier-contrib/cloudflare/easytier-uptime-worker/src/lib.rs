use axum::{Router, response::IntoResponse, routing::get};
use tower_http::cors::CorsLayer;
use tower_service::Service;
use worker::*;

#[derive(Clone)]
struct AppState;

fn router(_env: Env) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/healthz", get(health))
        .layer(CorsLayer::very_permissive())
        .with_state(AppState)
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    console_error_panic_hook::set_once();
    Ok(router(env).call(req).await?)
}

async fn root() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "name": "easytier-cloudflare-uptime",
        "status": "ok"
    }))
}

async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "success": true,
        "message": "Service is healthy"
    }))
}
