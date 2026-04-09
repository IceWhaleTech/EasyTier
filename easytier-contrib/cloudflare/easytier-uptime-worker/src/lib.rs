mod auth;
mod config;
mod db;
mod error;
mod models;
mod probe;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post, put},
};
use config::AppConfig;
use error::AppResult;
use models::{
    AdminLoginRequest, AdminNodeFilterParams, ApiResponse, CreateNodeRequest, HealthFilterParams,
    HealthStatsParams, NodeFilterParams, PaginationParams, UpdateNodeRequest,
};
use tower_http::cors::CorsLayer;
use tower_service::Service;
use worker::*;

#[derive(Clone)]
struct AppState {
    env: Env,
    config: AppConfig,
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/healthz", get(health))
        .route("/node/{id}", get(get_node_connect_url))
        .route("/api/nodes", get(get_nodes).post(create_node))
        .route("/api/nodes/{id}", get(get_node))
        .route("/api/tags", get(get_all_tags))
        .route("/api/test_connection", post(test_connection))
        .route("/api/nodes/{id}/health", get(get_node_health))
        .route("/api/nodes/{id}/health/stats", get(get_node_health_stats))
        .route("/api/admin/login", post(admin_login))
        .route("/api/admin/verify", get(admin_verify_token))
        .route("/api/admin/nodes", get(admin_get_nodes))
        .route("/api/admin/nodes/{id}/approve", put(admin_approve_node))
        .route("/api/admin/nodes/{id}/revoke", put(admin_revoke_node))
        .route(
            "/api/admin/nodes/{id}",
            put(admin_update_node).delete(admin_delete_node),
        )
        .layer(CorsLayer::very_permissive())
        .with_state(state)
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    console_error_panic_hook::set_once();
    let state = AppState {
        config: AppConfig::from_env(&env),
        env,
    };
    Ok(router(state).call(req).await?)
}

#[event(scheduled)]
async fn scheduled(event: ScheduledEvent, env: Env, ctx: ScheduleContext) {
    console_error_panic_hook::set_once();
    let config = AppConfig::from_env(&env);
    let cron = event.cron();

    ctx.wait_until(async move {
        if let Err(error) = db::run_scheduled_health_checks(&env, &config).await {
            console_error!("scheduled health check failed for cron {}: {}", cron, error);
        }
    });
}

async fn root() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "easytier-cloudflare-uptime",
        "status": "ok"
    }))
}

#[worker::send]
async fn health() -> Json<ApiResponse<String>> {
    Json(ApiResponse::message("Service is healthy"))
}

#[worker::send]
async fn get_nodes(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationParams>,
    Query(filters): Query<NodeFilterParams>,
) -> AppResult<Json<ApiResponse<models::PaginatedResponse<models::NodeResponse>>>> {
    let response = db::list_nodes(&state.env, &state.config, &pagination, &filters, true).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn get_node(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> AppResult<Json<ApiResponse<models::NodeResponse>>> {
    let response = db::get_node(&state.env, id, true).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn create_node(
    State(state): State<AppState>,
    Json(request): Json<CreateNodeRequest>,
) -> AppResult<Json<ApiResponse<models::NodeResponse>>> {
    request.validate()?;
    let response = db::create_node(&state.env, &request).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn test_connection(
    Json(request): Json<CreateNodeRequest>,
) -> AppResult<Json<ApiResponse<models::NodeResponse>>> {
    request.validate()?;
    let probe = probe::probe_request(&request).await?;
    if !probe.is_active {
        return Err(error::AppError::BadRequest(
            probe
                .error_message
                .clone()
                .unwrap_or_else(|| "connection test failed".to_string()),
        ));
    }
    Ok(Json(ApiResponse::success(
        models::NodeResponse::from_probe(&request, &probe),
    )))
}

#[worker::send]
async fn get_all_tags(State(state): State<AppState>) -> AppResult<Json<ApiResponse<Vec<String>>>> {
    let tags = db::get_all_tags(&state.env).await?;
    Ok(Json(ApiResponse::success(tags)))
}

#[worker::send]
async fn get_node_health(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(pagination): Query<PaginationParams>,
    Query(filters): Query<HealthFilterParams>,
) -> AppResult<Json<ApiResponse<models::PaginatedResponse<models::HealthRecordResponse>>>> {
    let response = db::get_node_health(&state.env, id, &pagination, &filters).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn get_node_health_stats(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(params): Query<HealthStatsParams>,
) -> AppResult<Json<ApiResponse<models::HealthStatsResponse>>> {
    let response = db::get_node_health_stats(&state.env, id, params.hours.unwrap_or(24)).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn get_node_connect_url(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> AppResult<String> {
    db::get_node_connect_url(&state.env, id).await
}

#[worker::send]
async fn admin_login(
    State(state): State<AppState>,
    Json(request): Json<AdminLoginRequest>,
) -> AppResult<Json<ApiResponse<models::AdminLoginResponse>>> {
    request.validate()?;
    let response = auth::login(&state.env, &state.config, &request.password).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn admin_verify_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse<String>>> {
    auth::require_admin(&state.env, &headers).await?;
    Ok(Json(ApiResponse::message("Token is valid")))
}

#[worker::send]
async fn admin_get_nodes(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationParams>,
    Query(filters): Query<AdminNodeFilterParams>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse<models::PaginatedResponse<models::NodeResponse>>>> {
    auth::require_admin(&state.env, &headers).await?;
    let response = db::list_admin_nodes(&state.env, &state.config, &pagination, &filters).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn admin_approve_node(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse<models::NodeResponse>>> {
    auth::require_admin(&state.env, &headers).await?;
    let response = db::set_node_approval(&state.env, id, true).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn admin_revoke_node(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse<models::NodeResponse>>> {
    auth::require_admin(&state.env, &headers).await?;
    let response = db::set_node_approval(&state.env, id, false).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn admin_update_node(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
    Json(request): Json<UpdateNodeRequest>,
) -> AppResult<Json<ApiResponse<models::NodeResponse>>> {
    auth::require_admin(&state.env, &headers).await?;
    request.validate()?;
    let response = db::update_node(&state.env, id, &request).await?;
    Ok(Json(ApiResponse::success(response)))
}

#[worker::send]
async fn admin_delete_node(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse<String>>> {
    auth::require_admin(&state.env, &headers).await?;
    db::delete_node(&state.env, id).await?;
    Ok(Json(ApiResponse::message("Node deleted successfully")))
}
