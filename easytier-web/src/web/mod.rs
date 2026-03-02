use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing, Router,
};
use axum_embed::ServeEmbed;
use easytier::common::scoped_task::ScopedTask;
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Embed assets for web dashboard, build frontend first
#[derive(RustEmbed, Clone)]
#[folder = "frontend/dist/"]
struct Assets;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ApiMetaResponse {
    api_host: String,
}

async fn handle_api_meta(State(api_host): State<Option<url::Url>>) -> impl IntoResponse {
    let body = if let Some(api_host) = api_host {
        format!(
            "window.apiMeta = {}",
            serde_json::to_string(&ApiMetaResponse {
                api_host: api_host.to_string()
            })
            .unwrap(),
        )
    } else {
        // Default to current origin so frontend can work behind gateway path prefixes.
        "window.apiMeta = { api_host: window.location.origin }".to_string()
    };

    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .header(header::PRAGMA, "no-cache")
        .header(header::EXPIRES, "0")
        .body(body)
        .unwrap()
}

pub fn build_router(api_host: Option<url::Url>) -> Router {
    let root_service = ServeEmbed::<Assets>::new();
    let prefixed_service = ServeEmbed::<Assets>::new();

    Router::new()
        // Keep root entry for direct service access.
        .route("/api_meta.js", routing::get(handle_api_meta))
        // Serve dashboard from gateway prefix.
        .route("/mt/api_meta.js", routing::get(handle_api_meta))
        .nest_service("/mt", prefixed_service)
        .fallback_service(root_service)
        .with_state(api_host)
}

pub struct WebServer {
    bind_addr: SocketAddr,
    router: Router,
    serve_task: Option<ScopedTask<()>>,
}

impl WebServer {
    pub async fn new(bind_addr: SocketAddr, router: Router) -> anyhow::Result<Self> {
        Ok(WebServer {
            bind_addr,
            router,
            serve_task: None,
        })
    }

    pub async fn start(self) -> Result<ScopedTask<()>, anyhow::Error> {
        let listener = TcpListener::bind(self.bind_addr).await?;
        let app = self.router;

        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        })
        .into();

        Ok(task)
    }
}
