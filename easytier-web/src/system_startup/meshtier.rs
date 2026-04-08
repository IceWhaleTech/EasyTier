use std::time::{Duration, Instant};

use easytier::common::scoped_task::ScopedTask;

use crate::client_manager::ClientManager;

const AUTO_REGISTER_MESHTIER_ON_START_ENV: &str =
    "ZIMAOS_EASYTIER_WEB_AUTO_REGISTER_MESHTIER_ON_START";
const AUTO_REGISTER_MESHTIER_WAIT_SECS_ENV: &str =
    "ZIMAOS_EASYTIER_WEB_AUTO_REGISTER_MESHTIER_WAIT_SECS";
const AUTO_REGISTER_MESHTIER_RETRY_SECS_ENV: &str =
    "ZIMAOS_EASYTIER_WEB_AUTO_REGISTER_MESHTIER_RETRY_SECS";

const DEFAULT_AUTO_REGISTER_MESHTIER_WAIT_SECS: u64 = 300;
const DEFAULT_AUTO_REGISTER_MESHTIER_RETRY_SECS: u64 = 3;

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

pub fn start_auto_register_task(client_mgr: std::sync::Arc<ClientManager>) -> ScopedTask<()> {
    ScopedTask::from(tokio::spawn(async move {
        if !parse_env_bool(AUTO_REGISTER_MESHTIER_ON_START_ENV, true) {
            tracing::info!(
                "{}=false, skip startup meshtier auto registration",
                AUTO_REGISTER_MESHTIER_ON_START_ENV
            );
            return;
        }

        let wait_secs = parse_env_u64(
            AUTO_REGISTER_MESHTIER_WAIT_SECS_ENV,
            DEFAULT_AUTO_REGISTER_MESHTIER_WAIT_SECS,
        );
        let retry_secs = parse_env_u64(
            AUTO_REGISTER_MESHTIER_RETRY_SECS_ENV,
            DEFAULT_AUTO_REGISTER_MESHTIER_RETRY_SECS,
        )
        .max(1);
        let deadline = Instant::now() + Duration::from_secs(wait_secs);

        loop {
            match crate::restful::meshtier::ensure_meshtier_instance_for_startup(
                client_mgr.as_ref(),
            )
            .await
            {
                Ok(true) => {
                    tracing::info!("startup meshtier auto registration finished");
                    return;
                }
                Ok(false) => {
                    tracing::debug!(
                        "startup meshtier auto registration skipped, prerequisites not ready yet"
                    );
                }
                Err(err) => {
                    tracing::warn!("startup meshtier auto registration failed: {err}");
                }
            }

            if Instant::now() >= deadline {
                tracing::warn!(
                    "startup meshtier auto registration timed out after {} seconds",
                    wait_secs
                );
                return;
            }

            tokio::time::sleep(Duration::from_secs(retry_secs)).await;
        }
    }))
}
