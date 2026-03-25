use std::{sync::Arc, time::Duration};

use crate::common::{config::PeerConfig, error::Error, global_ctx::ArcGlobalCtx, idn};

mod builtin;
mod file;
mod http;
pub mod manager;
mod peer_list;

use builtin::BuiltinPeerList;
use file::FilePeerList;
use http::HttpPeerList;
use peer_list::PeerList;

pub(crate) const MAX_DISCOVERY_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum PeerRef {
    Target(PeerTarget),
    List(PeerListRef),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeerTarget {
    peer: PeerConfig,
}

impl PeerTarget {
    pub fn peer(&self) -> &PeerConfig {
        &self.peer
    }

    pub fn into_peer(self) -> PeerConfig {
        self.peer
    }
}

#[derive(Debug, Clone)]
pub struct PeerListRef {
    inner: Arc<dyn PeerList>,
}

impl PeerListRef {
    pub fn default_builtin() -> Self {
        Self::new(BuiltinPeerList::default_root())
    }

    fn new<T>(list: T) -> Self
    where
        T: PeerList + 'static,
    {
        Self {
            inner: Arc::new(list),
        }
    }

    pub fn peer(&self) -> &PeerConfig {
        self.inner.peer()
    }

    pub fn refresh_interval(&self) -> Option<Duration> {
        self.inner.refresh_interval()
    }

    pub fn source_key(&self) -> String {
        self.peer().uri.to_string()
    }

    pub async fn resolve(&self, ctx: &ArcGlobalCtx) -> Result<Vec<PeerRef>, Error> {
        let discovered = self.inner.resolve(ctx).await?;

        if discovered.is_empty() {
            return Err(Error::InvalidUrl(format!(
                "no valid peer refs found in list: {}",
                self.peer().uri
            )));
        }

        Ok(discovered)
    }
}

impl PartialEq for PeerListRef {
    fn eq(&self, other: &Self) -> bool {
        self.peer() == other.peer()
    }
}

impl TryFrom<PeerConfig> for PeerRef {
    type Error = Error;

    fn try_from(peer: PeerConfig) -> Result<Self, Self::Error> {
        let normalized = PeerConfig {
            uri: idn::convert_idn_to_ascii(peer.uri)?,
            peer_public_key: peer.peer_public_key,
        };

        if let Some(list) = try_build_peer_list(normalized.clone())? {
            Ok(Self::List(list))
        } else {
            Ok(Self::Target(PeerTarget { peer: normalized }))
        }
    }
}

fn try_build_peer_list(peer: PeerConfig) -> Result<Option<PeerListRef>, Error> {
    if let Some(list) = BuiltinPeerList::try_from_peer(peer.clone()) {
        return Ok(Some(PeerListRef::new(list)));
    }
    if let Some(list) = FilePeerList::try_from_peer(peer.clone())? {
        return Ok(Some(PeerListRef::new(list)));
    }
    if let Some(list) = HttpPeerList::try_from_peer(peer)? {
        return Ok(Some(PeerListRef::new(list)));
    }
    Ok(None)
}

pub(crate) fn parse_peer_refs(input: &str) -> Vec<PeerRef> {
    input
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let uri = line.parse::<url::Url>().ok()?;
            let peer = PeerConfig {
                uri,
                peer_public_key: None,
            };
            PeerRef::try_from(peer).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_peer_refs_supports_targets_and_lists() {
        let refs = parse_peer_refs(
            r#"
            tcp://127.0.0.1:11010
            peerlist+file:///tmp/peers.txt
            # comment
            not-a-url
            "#,
        );

        assert_eq!(refs.len(), 2);
        match &refs[0] {
            PeerRef::Target(target) => {
                assert_eq!(target.peer().uri.as_str(), "tcp://127.0.0.1:11010")
            }
            _ => panic!("expected direct peer target"),
        }
        match &refs[1] {
            PeerRef::List(list) => {
                assert_eq!(list.peer().uri.as_str(), "peerlist+file:///tmp/peers.txt")
            }
            _ => panic!("expected nested peer list"),
        }
    }
}
