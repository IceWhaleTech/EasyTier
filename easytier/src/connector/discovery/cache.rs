use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use tokio::sync::Mutex;

use crate::{
    common::{config::NetworkIdentity, error::Error, global_ctx::ArcGlobalCtx},
    tunnel::generate_digest_from_str,
};

use super::{parse_peer_refs, PeerListRef, PeerRef};

#[derive(Clone)]
pub(crate) struct PeerListCacheStore {
    root_dir: PathBuf,
    save_lock: Arc<Mutex<()>>,
}

impl PeerListCacheStore {
    pub(crate) fn new(root_dir: Option<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.unwrap_or_else(default_store_root),
            save_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) async fn load(
        &self,
        global_ctx: &ArcGlobalCtx,
        list: &PeerListRef,
    ) -> Result<Option<Vec<PeerRef>>, Error> {
        let path = self.scope_file_path(global_ctx, list);
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };

        let refs = parse_peer_refs(&content);
        if refs.is_empty() {
            return Ok(None);
        }

        Ok(Some(refs))
    }

    pub(crate) async fn save(
        &self,
        global_ctx: &ArcGlobalCtx,
        list: &PeerListRef,
        refs: &[PeerRef],
    ) -> Result<(), Error> {
        if refs.is_empty() {
            return Ok(());
        }

        let _guard = self.save_lock.lock().await;
        let path = self.scope_file_path(global_ctx, list);
        tracing::info!(path = ?path, "saving peer refs");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = refs.iter().map(PeerRef::key).collect::<Vec<_>>().join("\n");
        tokio::fs::write(&path, content)
            .await
            .with_context(|| format!("writing peer list cache failed: {}", path.display()))?;
        Ok(())
    }

    fn scope_file_path(&self, global_ctx: &ArcGlobalCtx, list: &PeerListRef) -> PathBuf {
        let identity = global_ctx.get_network_identity();
        let scope_name = sanitize_scope_name(&identity.network_name);
        let network_digest = network_digest_hex(&identity);
        let source_digest = source_digest_hex(&list.source_key());

        self.root_dir
            .join("peer-list-cache")
            .join(format!("{}-{}", scope_name, network_digest))
            .join(format!("{}.txt", source_digest))
    }
}

fn default_store_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

fn sanitize_scope_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

fn network_digest_hex(identity: &NetworkIdentity) -> String {
    let digest = identity.network_secret_digest.unwrap_or_else(|| {
        let mut digest = [0u8; 32];
        generate_digest_from_str(
            &identity.network_name,
            identity.network_secret.as_deref().unwrap_or_default(),
            &mut digest,
        );
        digest
    });
    digest_hex(&digest)
}

fn source_digest_hex(source_key: &str) -> String {
    let mut digest = [0u8; 32];
    generate_digest_from_str("peer-list-cache", source_key, &mut digest);
    digest_hex(&digest)
}

fn digest_hex(digest: &[u8]) -> String {
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::PeerConfig;
    use crate::common::global_ctx::tests::get_mock_global_ctx_with_network;
    use crate::connector::discovery::file::FilePeerList;

    #[tokio::test]
    async fn cache_roundtrip() {
        let root =
            std::env::temp_dir().join(format!("easytier-list-cache-test-{}", uuid::Uuid::new_v4()));
        let store = PeerListCacheStore::new(Some(root.clone()));
        let ctx = get_mock_global_ctx_with_network(Some(NetworkIdentity::new(
            "cache-test".to_string(),
            "secret".to_string(),
        )));
        let list = PeerListRef::new(
            FilePeerList::try_from_peer(PeerConfig {
                uri: "peerlist+file:///tmp/cache.txt".parse().unwrap(),
                peer_public_key: None,
            })
            .unwrap()
            .unwrap(),
        );
        let refs = vec![PeerRef::try_from(PeerConfig {
            uri: "tcp://127.0.0.1:11010".parse().unwrap(),
            peer_public_key: None,
        })
        .unwrap()];

        store.save(&ctx, &list, &refs).await.unwrap();
        let loaded = store.load(&ctx, &list).await.unwrap().unwrap();
        assert_eq!(loaded, refs);

        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
