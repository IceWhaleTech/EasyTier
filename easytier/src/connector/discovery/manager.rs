use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use dashmap::DashSet;

use crate::{
    common::{error::Error, global_ctx::ArcGlobalCtx, scoped_task::ScopedTask},
    connector::manual::ManualConnectorManager,
};

use super::{
    cache::PeerListCacheStore, PeerListCachePolicy, PeerListRef, PeerRef, PeerTarget,
    MAX_DISCOVERY_DEPTH,
};

struct DiscoveryState {
    global_ctx: ArcGlobalCtx,
    conn_manager: Arc<ManualConnectorManager>,
    cache_store: PeerListCacheStore,
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
            cache_store: PeerListCacheStore::new(None),
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
                let source = peer_ref.key();
                tracing::warn!(?err, source = %source, "initial peer discovery failed");
            }
        }

        Ok(manager)
    }

    async fn sync_bootstrap_ref(&self, peer_ref: PeerRef) -> Result<(), Error> {
        let targets = resolve_peer_ref(self.state.clone(), peer_ref).await?;
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
    let targets = resolve_peer_ref(state.clone(), PeerRef::List(list)).await?;
    register_peer_targets(&state.conn_manager, targets).await
}

async fn resolve_peer_ref(
    state: Arc<DiscoveryState>,
    peer_ref: PeerRef,
) -> Result<Vec<PeerTarget>, Error> {
    let mut visited_lists = HashSet::new();
    let mut resolved = Vec::new();
    let mut stack = vec![(peer_ref, 0usize)];

    while let Some((peer_ref, depth)) = stack.pop() {
        if depth > MAX_DISCOVERY_DEPTH {
            return Err(Error::AnyhowError(anyhow::anyhow!(
                "peer discovery nesting exceeded maximum depth of {}",
                MAX_DISCOVERY_DEPTH
            )));
        }

        match peer_ref {
            PeerRef::Target(target) => resolved.push(target),
            PeerRef::List(list) => {
                expand_peer_list(&state, &mut stack, &mut visited_lists, list, depth).await?;
            }
        }
    }

    Ok(resolved)
}

async fn expand_peer_list(
    state: &Arc<DiscoveryState>,
    stack: &mut Vec<(PeerRef, usize)>,
    visited_lists: &mut HashSet<String>,
    list: PeerListRef,
    depth: usize,
) -> Result<(), Error> {
    state.ensure_refresh_task(&list);

    let source_key = list.source_key();
    if !visited_lists.insert(source_key.clone()) {
        tracing::warn!(source = %source_key, "skipping recursive peer list");
        return Ok(());
    }

    match resolve_list_refs(state, &list).await {
        Ok(discovered) => {
            for child in discovered.into_iter().rev() {
                stack.push((child, depth + 1));
            }
            Ok(())
        }
        Err(err) if depth == 0 => Err(err),
        Err(err) => {
            tracing::warn!(?err, source = %source_key, "nested peer discovery failed");
            Ok(())
        }
    }
}

async fn resolve_list_refs(
    state: &Arc<DiscoveryState>,
    list: &PeerListRef,
) -> Result<Vec<PeerRef>, Error> {
    let cached = load_cached_refs(state, list).await;

    match list.resolve(&state.global_ctx).await {
        Ok(discovered) => {
            persist_cached_refs(state, list, &discovered).await;
            Ok(discovered)
        }
        Err(err) => {
            if let Some(cached) = cached {
                tracing::warn!(
                    ?err,
                    source = %list.source_key(),
                    "peer list resolve failed, using cached refs"
                );
                Ok(cached)
            } else {
                Err(err)
            }
        }
    }
}

async fn load_cached_refs(state: &Arc<DiscoveryState>, list: &PeerListRef) -> Option<Vec<PeerRef>> {
    if list.cache_policy() != PeerListCachePolicy::DynamicFile {
        return None;
    }

    match state.cache_store.load(&state.global_ctx, list).await {
        Ok(cached) => cached,
        Err(err) => {
            tracing::warn!(?err, source = %list.source_key(), "loading peer list cache failed");
            None
        }
    }
}

async fn persist_cached_refs(state: &Arc<DiscoveryState>, list: &PeerListRef, refs: &[PeerRef]) {
    if list.cache_policy() != PeerListCachePolicy::DynamicFile {
        return;
    }

    if let Err(err) = state.cache_store.save(&state.global_ctx, list, refs).await {
        tracing::warn!(?err, source = %list.source_key(), "writing peer list cache failed");
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
            cache_store: PeerListCacheStore::new(None),
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

        let resolved = resolve_peer_ref(state, peer_ref).await.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].peer().uri.as_str(), "tcp://127.0.0.1:11010");

        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
