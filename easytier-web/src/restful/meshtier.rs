use std::{collections::BTreeSet, time::Duration};

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use easytier::{
    launcher::{NetworkConfig, NetworkingMethod},
    proto::{
        api::manage::{CollectNetworkInfoResponse, NetworkInstanceRunningInfo},
        rpc_types::error::Error as RpcError,
    },
    rpc_service::remote_client::{
        ListNetworkProps, PersistentConfig, RemoteClientError, RemoteClientManager, Storage,
    },
};
use reqwest::{Client, Method, Url};
use sea_orm::DbErr;
use serde::{Deserialize, Serialize};
use tokio::time::{interval, timeout};

use crate::{
    client_manager::ClientManager,
    db::{entity::user_running_network_configs, UserIdInDb},
};

use super::{convert_db_error, other_error, AppState, AppStateInner, Error, HttpHandleError};

const DEFAULT_ZEROTIER_SERVICE_URL: &str = "http://localhost/";
const DEFAULT_HOSTNAME: &str = "mesh-node";
const EASYTIER_WAIT_TIMEOUT_SECS: u64 = 30;
const EASYTIER_POLL_INTERVAL_MS: u64 = 300;
const EASYTIER_ADMIN_TOKEN: &str = "admin";

const MESH_TIER_PREFIX: [u8; 8] = *b"meshtier";

type SessionIdentity = (UserIdInDb, uuid::Uuid);
type ApiResult = Result<Json<MeshResponse>, HttpHandleError>;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MeshStatus {
    Online,
    Offline,
    Reset,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MeshResponse {
    pub id: String,
    pub name: String,
    pub status: MeshStatus,
    pub ip: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MeshStatusRequest {
    status: MeshStatus,
}

#[derive(Debug, Serialize)]
struct ZeroTierStatusRequest {
    status: MeshStatus,
}

#[derive(Debug, Clone)]
struct ZeroTierClient {
    client: Client,
    base_url: Url,
}

pub fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/mt/ping", get(api_ping))
        .route("/mt/info", get(api_mesh_info))
        .route("/mt/status", put(api_mesh_status))
        .route("/mt/connect", post(api_mesh_connect))
        .route("/mt/disconnect", post(api_mesh_disconnect))
        .route("/api/v1/mesh/ping", get(api_ping))
        .route("/api/v1/mesh/info", get(api_mesh_info))
        .route("/api/v1/mesh/status", put(api_mesh_status))
        .route("/api/v1/mesh/connect", post(api_mesh_connect))
        .route("/api/v1/mesh/disconnect", post(api_mesh_disconnect))
}

async fn api_ping(State(client_mgr): AppState) -> Result<&'static str, HttpHandleError> {
    pick_admin_identity(client_mgr.as_ref()).await?;
    Ok("pong")
}

async fn api_mesh_info(State(client_mgr): AppState) -> ApiResult {
    let runtime = MeshRuntime::from_request(client_mgr.as_ref()).await?;
    let response = runtime.zerotier.get_info().await.map_err(internal_error)?;
    Ok(Json(response))
}

async fn api_mesh_status(
    State(client_mgr): AppState,
    Json(payload): Json<MeshStatusRequest>,
) -> ApiResult {
    let response = match payload.status {
        MeshStatus::Online => do_connect(client_mgr.as_ref()).await?,
        MeshStatus::Offline => do_disconnect(client_mgr.as_ref()).await?,
        MeshStatus::Reset => do_reset(client_mgr.as_ref()).await?,
    };
    Ok(Json(response))
}

async fn api_mesh_connect(State(client_mgr): AppState) -> ApiResult {
    let response = do_connect(client_mgr.as_ref()).await?;
    Ok(Json(response))
}

async fn api_mesh_disconnect(State(client_mgr): AppState) -> ApiResult {
    let response = do_disconnect(client_mgr.as_ref()).await?;
    Ok(Json(response))
}

async fn do_connect(client_mgr: &ClientManager) -> Result<MeshResponse, HttpHandleError> {
    let runtime = MeshRuntime::from_request(client_mgr).await?;
    runtime.zerotier.connect().await.map_err(internal_error)?;

    let zt_info = runtime
        .zerotier
        .wait_ip_info()
        .await
        .map_err(internal_error)?;
    let instance_id = zerotier_id_to_uuid(&zt_info.id)
        .map_err(|e| internal_error(format!("invalid zerotier id: {e}")))?;

    // Keep meshtier as a single instance: always remove old meshtier instances before create.
    clear_meshtier_networks(client_mgr, runtime.identity).await?;

    let cfg = default_mesh_network_config(instance_id, &zt_info.id, &zt_info.id);
    client_mgr
        .handle_run_network_instance(runtime.identity, cfg, true)
        .await
        .map_err(convert_remote_error)?;

    // Keep startup checks for EasyTier instance, but return ZT-compatible payload for API callers.
    wait_easytier_online(client_mgr, runtime.identity, instance_id).await?;

    match runtime.zerotier.get_info().await {
        Ok(info) => Ok(info),
        Err(err) => {
            tracing::warn!(
                "zerotier get_info after connect failed, fallback to wait_ip_info response: {err}"
            );
            Ok(zt_info)
        }
    }
}

async fn do_disconnect(client_mgr: &ClientManager) -> Result<MeshResponse, HttpHandleError> {
    let runtime = MeshRuntime::from_request(client_mgr).await?;
    let zt_response = runtime
        .zerotier
        .disconnect()
        .await
        .map_err(internal_error)?;

    clear_meshtier_networks(client_mgr, runtime.identity).await?;

    Ok(zt_response)
}

async fn do_reset(client_mgr: &ClientManager) -> Result<MeshResponse, HttpHandleError> {
    let runtime = MeshRuntime::from_request(client_mgr).await?;
    clear_meshtier_networks(client_mgr, runtime.identity).await?;
    runtime
        .zerotier
        .set_status(MeshStatus::Reset)
        .await
        .map_err(internal_error)
}

async fn wait_easytier_online(
    client_mgr: &ClientManager,
    identity: SessionIdentity,
    inst_id: uuid::Uuid,
) -> Result<MeshResponse, HttpHandleError> {
    let mut ticker = interval(Duration::from_millis(EASYTIER_POLL_INTERVAL_MS));
    timeout(Duration::from_secs(EASYTIER_WAIT_TIMEOUT_SECS), async {
        loop {
            ticker.tick().await;
            let response = client_mgr
                .handle_collect_network_info(identity, Some(vec![inst_id]))
                .await;

            let Ok(info) = response else {
                continue;
            };

            let Some(node_info) = find_network_info(info, &inst_id) else {
                continue;
            };

            if !node_info.running {
                if let Some(error_msg) = node_info
                    .error_msg
                    .clone()
                    .filter(|msg| !msg.trim().is_empty())
                {
                    break Err(internal_error(format!(
                        "easytier instance startup failed: {error_msg}"
                    )));
                }
                continue;
            }

            break Ok(build_easytier_response(inst_id, node_info));
        }
    })
    .await
    .map_err(|_| internal_error("wait easytier online timeout"))?
}

async fn clear_meshtier_networks(
    client_mgr: &ClientManager,
    identity: SessionIdentity,
) -> Result<(), HttpHandleError> {
    let mut network_ids = BTreeSet::new();

    if let Ok(info) = client_mgr.handle_collect_network_info(identity, None).await {
        add_running_meshtier_network_ids(&mut network_ids, info);
    }

    let saved_networks: Vec<user_running_network_configs::Model> = client_mgr
        .get_storage()
        .list_network_configs(identity, ListNetworkProps::All)
        .await
        .map_err(convert_db_error)?;

    for network in saved_networks {
        if let Ok(inst_id) = uuid::Uuid::parse_str(network.get_network_inst_id()) {
            if is_meshtier_instance(&inst_id) {
                network_ids.insert(inst_id);
            }
        }
    }

    if network_ids.is_empty() {
        return Ok(());
    }

    client_mgr
        .handle_remove_network_instances(identity, network_ids.into_iter().collect())
        .await
        .map_err(convert_remote_error)
}

fn add_running_meshtier_network_ids(
    network_ids: &mut BTreeSet<uuid::Uuid>,
    response: CollectNetworkInfoResponse,
) {
    let Some(info_map) = response.info else {
        return;
    };

    for inst_id in info_map.map.keys() {
        if let Ok(uuid) = uuid::Uuid::parse_str(inst_id) {
            if is_meshtier_instance(&uuid) {
                network_ids.insert(uuid);
            }
        }
    }
}

fn is_meshtier_instance(inst_id: &uuid::Uuid) -> bool {
    inst_id.as_bytes()[..8] == MESH_TIER_PREFIX
}

fn find_network_info(
    response: CollectNetworkInfoResponse,
    inst_id: &uuid::Uuid,
) -> Option<NetworkInstanceRunningInfo> {
    let info_map = response.info?;
    info_map
        .map
        .get(&inst_id.to_string())
        .cloned()
        .or_else(|| info_map.map.into_values().next())
}

fn build_easytier_response(inst_id: uuid::Uuid, info: NetworkInstanceRunningInfo) -> MeshResponse {
    let status = if info.running {
        MeshStatus::Online
    } else {
        MeshStatus::Offline
    };

    let (name, ip) = if let Some(my_node) = info.my_node_info {
        let hostname = if my_node.hostname.trim().is_empty() {
            "easytier".to_string()
        } else {
            my_node.hostname
        };

        let ip = my_node
            .virtual_ipv4
            .and_then(|inet| inet.address)
            .map(|ipv4| std::net::Ipv4Addr::from(ipv4.addr).to_string());
        (hostname, ip)
    } else {
        ("easytier".to_string(), None)
    };

    MeshResponse {
        id: inst_id.to_string(),
        name,
        status,
        ip,
    }
}

fn default_mesh_network_config(
    instance_id: uuid::Uuid,
    network_name: &str,
    network_secret: &str,
) -> NetworkConfig {
    NetworkConfig {
        instance_id: Some(instance_id.to_string()),
        dhcp: Some(true),
        virtual_ipv4: Some(String::new()),
        network_length: Some(16),
        hostname: Some(default_hostname()),
        network_name: Some(network_name.to_string()),
        network_secret: Some(network_secret.to_string()),
        networking_method: Some(NetworkingMethod::Manual as i32),
        peer_urls: vec!["wss://et.icewhale.io".to_string()],
        listener_urls: vec![
            "tcp://0.0.0.0:11010".to_string(),
            "udp://0.0.0.0:11010".to_string(),
            "ws://0.0.0.0:11011".to_string(),
        ],
        latency_first: Some(true),
        bind_device: Some(true),
        multi_thread: Some(true),
        disable_encryption: Some(true),
        ..Default::default()
    }
}

fn default_hostname() -> String {
    let system_hostname = gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string();
    if !system_hostname.is_empty() {
        return system_hostname;
    }

    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_HOSTNAME.to_string())
}

fn zerotier_id_to_uuid(zt_id: &str) -> Result<uuid::Uuid, String> {
    if zt_id.len() != 16 {
        return Err(format!(
            "invalid ZeroTier id length: expected 16 hex chars, got {}",
            zt_id.len()
        ));
    }

    let value =
        u64::from_str_radix(zt_id, 16).map_err(|_| format!("invalid ZeroTier hex id: {zt_id}"))?;
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes[..8].copy_from_slice(&MESH_TIER_PREFIX);
    uuid_bytes[8..].copy_from_slice(&value.to_be_bytes());

    Ok(uuid::Uuid::from_bytes(uuid_bytes))
}

#[allow(dead_code)]
fn uuid_to_zerotier_id(instance_id: &str) -> Result<String, String> {
    let parsed = uuid::Uuid::parse_str(instance_id).map_err(|e| format!("invalid uuid: {e}"))?;
    let bytes = parsed.as_bytes();
    if bytes[..8] != MESH_TIER_PREFIX {
        return Err("uuid is not encoded by meshtier zerotier codec".to_string());
    }

    let mut zt = [0u8; 8];
    zt.copy_from_slice(&bytes[8..16]);
    Ok(format!("{:016x}", u64::from_be_bytes(zt)))
}

fn parse_zerotier_base_url() -> Result<Url, HttpHandleError> {
    let raw = std::env::var("ZEROTIER_SERVICE_URL")
        .unwrap_or_else(|_| DEFAULT_ZEROTIER_SERVICE_URL.to_string());
    raw.parse::<Url>()
        .map_err(|e| internal_error(format!("Invalid ZEROTIER_SERVICE_URL: {e}")))
}

fn internal_error(err: impl std::fmt::Display) -> HttpHandleError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(Error {
            message: err.to_string(),
        }),
    )
}

fn convert_rpc_error(err: RpcError) -> HttpHandleError {
    let status_code = match &err {
        RpcError::ExecutionError(_) => StatusCode::BAD_REQUEST,
        RpcError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status_code,
        Json(Error {
            message: format!("{:?}", err),
        }),
    )
}

fn convert_remote_error(err: RemoteClientError<DbErr>) -> HttpHandleError {
    match err {
        RemoteClientError::PersistentError(db_err) => convert_db_error(db_err),
        RemoteClientError::RpcError(rpc_err) => convert_rpc_error(rpc_err),
        RemoteClientError::ClientNotFound => (
            StatusCode::NOT_FOUND,
            Json(other_error("EasyTier client not found")),
        ),
        RemoteClientError::NotFound(msg) => (StatusCode::NOT_FOUND, Json(other_error(msg))),
        RemoteClientError::Other(msg) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(other_error(msg)))
        }
    }
}

#[derive(Debug)]
struct MeshRuntime {
    identity: SessionIdentity,
    zerotier: ZeroTierClient,
}

impl MeshRuntime {
    async fn from_request(client_mgr: &ClientManager) -> Result<Self, HttpHandleError> {
        let identity = pick_admin_identity(client_mgr).await?;
        let zerotier = ZeroTierClient::new(parse_zerotier_base_url()?);
        Ok(Self { identity, zerotier })
    }
}

async fn pick_admin_identity(
    client_mgr: &ClientManager,
) -> Result<SessionIdentity, HttpHandleError> {
    let mut sessions = client_mgr.list_sessions().await;
    sessions.sort_by(|a, b| a.machine_id.to_string().cmp(&b.machine_id.to_string()));
    let Some(admin) = sessions
        .into_iter()
        .find(|s| s.token == EASYTIER_ADMIN_TOKEN)
    else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(other_error(format!(
                "no online easytier-core session with token '{EASYTIER_ADMIN_TOKEN}'",
            ))),
        ));
    };
    Ok((admin.user_id, admin.machine_id))
}

impl ZeroTierClient {
    fn new(base_url: Url) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    fn api_url(&self, path: &str) -> anyhow::Result<Url> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .map_err(|e| anyhow::anyhow!("invalid zerotier api url: {e}"))
    }

    async fn get_info(&self) -> anyhow::Result<MeshResponse> {
        let url = self.api_url("/v2/zimaos/zt/info")?;
        let response = self
            .client
            .request(Method::GET, url)
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn set_status(&self, status: MeshStatus) -> anyhow::Result<MeshResponse> {
        let url = self.api_url("/v2/zimaos/zt/status")?;
        let response = self
            .client
            .request(Method::PUT, url)
            .json(&ZeroTierStatusRequest { status })
            .send()
            .await?
            .error_for_status()?;
        Ok(response.json().await?)
    }

    async fn connect(&self) -> anyhow::Result<MeshResponse> {
        let connect_info = self.get_info().await;
        match connect_info {
            Ok(MeshResponse {
                status: MeshStatus::Online,
                ..
            }) => connect_info,
            Ok(MeshResponse {
                status: MeshStatus::Reset,
                ..
            }) => connect_info,
            _ => self.set_status(MeshStatus::Online).await,
        }
    }

    async fn disconnect(&self) -> anyhow::Result<MeshResponse> {
        self.set_status(MeshStatus::Offline).await
    }

    async fn wait_ip_info(&self) -> anyhow::Result<MeshResponse> {
        let mut ticker = interval(Duration::from_secs(1));
        timeout(Duration::from_secs(30), async {
            loop {
                ticker.tick().await;
                let connect_info = self.get_info().await?;
                if connect_info.ip.is_some() {
                    break Ok(connect_info);
                }
            }
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::{uuid_to_zerotier_id, zerotier_id_to_uuid};

    #[test]
    fn codec_roundtrip_is_lossless() {
        let sample = "fb8afe3192e4d274";
        let uuid = zerotier_id_to_uuid(sample).unwrap();
        let decoded = uuid_to_zerotier_id(&uuid.to_string()).unwrap();
        assert_eq!(decoded, sample);
    }

    #[test]
    fn codec_rejects_invalid_len() {
        assert!(zerotier_id_to_uuid("abc").is_err());
    }
}
