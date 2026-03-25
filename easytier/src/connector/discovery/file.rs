use std::{path::PathBuf, time::Duration};

use anyhow::Context;
use async_trait::async_trait;

use crate::common::{config::PeerConfig, error::Error, global_ctx::ArcGlobalCtx};

use super::{parse_peer_refs, peer_list::PeerList, PeerRef};

const FILE_LIST_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FilePeerList {
    peer: PeerConfig,
    path: PathBuf,
}

impl FilePeerList {
    pub(crate) fn try_from_peer(peer: PeerConfig) -> Result<Option<Self>, Error> {
        if peer.uri.scheme() != "peerlist+file" {
            return Ok(None);
        }

        let target = peer
            .uri
            .as_str()
            .replacen("peerlist+file://", "file://", 1)
            .parse::<url::Url>()
            .map_err(|_| Error::InvalidUrl(format!("invalid peer list file url: {}", peer.uri)))?;
        let path = target
            .to_file_path()
            .map_err(|_| Error::InvalidUrl(format!("invalid peer list file path: {}", peer.uri)))?;

        Ok(Some(Self { peer, path }))
    }
}

#[async_trait]
impl PeerList for FilePeerList {
    fn peer(&self) -> &PeerConfig {
        &self.peer
    }

    fn refresh_interval(&self) -> Option<Duration> {
        Some(FILE_LIST_REFRESH_INTERVAL)
    }

    async fn resolve(&self, _ctx: &ArcGlobalCtx) -> Result<Vec<PeerRef>, Error> {
        let body = tokio::fs::read_to_string(&self.path)
            .await
            .with_context(|| format!("reading peer list file failed: {}", self.path.display()))?;
        Ok(parse_peer_refs(&body))
    }
}
