use super::network::convert_error;
use crate::db::UserIdInDb;
use crate::default::default_network::{IceWhaleNetworkConfig, NetworkConfig};
use crate::restful::users::AuthSession;
use crate::restful::{other_error, AppState, AppStateInner, Error, HttpHandleError};
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use axum_login::AuthUser;
use easytier::proto::common::Void;
use easytier::rpc_service::remote_client::RemoteClientManager;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct RunDefaultNetworkReq {
    config: IceWhaleNetworkConfig,
    save: bool,
}

pub struct NetworkDefaultApi;

impl NetworkDefaultApi {
    fn get_user_id(auth_session: &AuthSession) -> Result<UserIdInDb, (StatusCode, Json<Error>)> {
        let Some(user_id) = auth_session.user.as_ref().map(|x| x.id()) else {
            return Err((
                StatusCode::UNAUTHORIZED,
                other_error("No user id found".to_string()).into(),
            ));
        };
        Ok(user_id)
    }

    async fn handle_run_network_default_instance(
        auth_session: AuthSession,
        State(client_mgr): AppState,
        Path(machine_id): Path<uuid::Uuid>,
        Json(payload): Json<RunDefaultNetworkReq>,
    ) -> Result<Json<Void>, HttpHandleError> {
        let default_config = NetworkConfig::try_from(payload.config).unwrap();
        client_mgr
            .handle_run_network_instance(
                (Self::get_user_id(&auth_session)?, machine_id),
                default_config,
                true,
            )
            .await
            .map_err(convert_error)?;
        Ok(Void::default().into())
    }


    pub fn build_route() -> Router<AppStateInner> {
        Router::new().route(
            "/api/v1/default/machines/:machine-id/networks",
            post(Self::handle_run_network_default_instance),
        )
    }
}

