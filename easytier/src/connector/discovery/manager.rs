use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use async_recursion::async_recursion;
use dashmap::DashSet;

use crate::{
    common::{error::Error, global_ctx::ArcGlobalCtx, scoped_task::ScopedTask},
    connector::manual::ManualConnectorManager,
};

use super::{PeerListRef, PeerRef, PeerTarget, MAX_DISCOVERY_DEPTH};

struct DiscoveryState {
    global_ctx: ArcGlobalCtx,
    conn_manager: Arc<ManualConnectorManager>,
    refresh_tasks: Mutex<Vec<ScopedTask<()>>>,
    scheduled_lists: DashSet<String>,
}

pub struct PeerDiscoveryManager {
    state: Arc<DiscoveryState>,
}

impl PeerDiscoveryManager {
    pub async fn new(
        global_ctx: ArcGlobalCtx,
        conn_manager: Arc<ManualConnectorManager>,
    ) -> Result<Self, Error> {
        let state = Arc::new(DiscoveryState {
            global_ctx: global_ctx.clone(),
            conn_manager,
            refresh_tasks: Mutex::new(Vec::new()),
            scheduled_lists: DashSet::new(),
        });
        let manager = Self { state };

        let bootstrap_refs = global_ctx
            .config
            .get_peers()
            .into_iter()
            .map(PeerRef::try_from)
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "parsing configured peer refs failed")?;

        for peer_ref in bootstrap_refs
            .into_iter()
            .chain(std::iter::once(PeerRef::List(
                PeerListRef::default_builtin(),
            )))
        {
            if let Err(err) = manager.sync_bootstrap_ref(peer_ref.clone()).await {
                let source = match &peer_ref {
                    PeerRef::Target(target) => target.peer().uri.to_string(),
                    PeerRef::List(list) => list.peer().uri.to_string(),
                };
                tracing::warn!(?err, source = %source, "initial peer discovery failed");
            }
        }

        Ok(manager)
    }

    async fn sync_bootstrap_ref(&self, peer_ref: PeerRef) -> Result<(), Error> {
        let targets =
            resolve_peer_ref(self.state.clone(), peer_ref, &mut HashSet::new(), 0).await?;
        register_peer_targets(&self.state.conn_manager, targets).await
    }
}

impl DiscoveryState {
    fn ensure_refresh_task(self: &Arc<Self>, list: &PeerListRef) {
        let Some(interval) = list.refresh_interval() else {
            return;
        };

        let source_key = list.source_key();
        if !self.scheduled_lists.insert(source_key.clone()) {
            return;
        }

        let state = self.clone();
        let list = list.clone();
        let source = list.peer().uri.clone();
        let task = ScopedTask::from(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(err) = sync_list_ref(state.clone(), list.clone()).await {
                    tracing::warn!(?err, source = %source, "peer list refresh failed");
                }
            }
        }));
        self.refresh_tasks.lock().unwrap().push(task);
    }
}

async fn register_peer_targets(
    conn_manager: &Arc<ManualConnectorManager>,
    targets: Vec<PeerTarget>,
) -> Result<(), Error> {
    let mut seen = HashSet::new();
    for target in targets {
        let peer = target.into_peer();
        if !seen.insert(peer.uri.to_string()) {
            continue;
        }
        conn_manager.add_connector_by_url(peer.uri).await?;
    }
    Ok(())
}

async fn sync_list_ref(state: Arc<DiscoveryState>, list: PeerListRef) -> Result<(), Error> {
    let targets =
        resolve_peer_ref(state.clone(), PeerRef::List(list), &mut HashSet::new(), 0).await?;
    register_peer_targets(&state.conn_manager, targets).await
}

#[async_recursion]
async fn resolve_peer_ref(
    state: Arc<DiscoveryState>,
    peer_ref: PeerRef,
    visited_lists: &mut HashSet<String>,
    depth: usize,
) -> Result<Vec<PeerTarget>, Error> {
    if depth > MAX_DISCOVERY_DEPTH {
        return Err(Error::AnyhowError(anyhow::anyhow!(
            "peer discovery nesting exceeded maximum depth of {}",
            MAX_DISCOVERY_DEPTH
        )));
    }

    match peer_ref {
        PeerRef::Target(target) => Ok(vec![target]),
        PeerRef::List(list) => {
            state.ensure_refresh_task(&list);

            let source_key = list.source_key();
            if !visited_lists.insert(source_key.clone()) {
                tracing::warn!(source = %source_key, "skipping recursive peer list");
                return Ok(Vec::new());
            }

            let discovered = list.resolve(&state.global_ctx).await?;
            let mut resolved = Vec::new();
            for child in discovered {
                let source = match &child {
                    PeerRef::Target(target) => target.peer().uri.to_string(),
                    PeerRef::List(list) => list.peer().uri.to_string(),
                };
                match resolve_peer_ref(state.clone(), child, visited_lists, depth + 1).await {
                    Ok(child_targets) => resolved.extend(child_targets),
                    Err(err) => {
                        tracing::warn!(?err, source = %source, "nested peer discovery failed");
                    }
                }
            }

            Ok(resolved)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{config::PeerConfig, global_ctx::tests::get_mock_global_ctx};

    #[tokio::test]
    async fn resolve_nested_file_lists() {
        let root =
            std::env::temp_dir().join(format!("easytier-discovery-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let nested = root.join("nested.txt");
        let outer = root.join("outer.txt");
        let nested_url = url::Url::from_file_path(&nested).unwrap();
        let outer_url = url::Url::from_file_path(&outer).unwrap();
        tokio::fs::write(&nested, "tcp://127.0.0.1:11010\n")
            .await
            .unwrap();
        tokio::fs::write(
            &outer,
            format!(
                "{}\n",
                nested_url
                    .as_str()
                    .replacen("file://", "peerlist+file://", 1)
            ),
        )
        .await
        .unwrap();

        let ctx = get_mock_global_ctx();
        let state = Arc::new(DiscoveryState {
            global_ctx: ctx,
            conn_manager: Arc::new(ManualConnectorManager::new(
                get_mock_global_ctx(),
                crate::peers::tests::create_mock_peer_manager().await,
            )),
            refresh_tasks: Mutex::new(Vec::new()),
            scheduled_lists: DashSet::new(),
        });

        let peer_ref = PeerRef::try_from(PeerConfig {
            uri: outer_url
                .as_str()
                .replacen("file://", "peerlist+file://", 1)
                .parse()
                .unwrap(),
            peer_public_key: None,
        })
        .unwrap();

        let resolved = resolve_peer_ref(state, peer_ref, &mut HashSet::new(), 0)
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].peer().uri.as_str(), "tcp://127.0.0.1:11010");

        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
