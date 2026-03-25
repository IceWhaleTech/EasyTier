use async_trait::async_trait;

use crate::common::{config::PeerConfig, error::Error, global_ctx::ArcGlobalCtx};

use super::{parse_peer_refs, peer_list::PeerList, PeerRef};

const BUILTIN_LIST_URI: &str = "builtin://default";
const BUILTIN_PEER_REFS: &[&str] = &[
    "tcp://remote-eu-central-1a.icewhale.io:11010",
    "wss://remote-eu-central-1a.icewhale.io:11012",
    "wss://remote-us-east-2a.icewhale.io:11012",
    "tcp://remote-us-east-2a.icewhale.io:11010",
    "tcp://remote-ap-northeast-1a.icewhale.io:11010",
    "wss://remote-ap-northeast-1a.icewhale.io:11012",
    "udp://remote-eu-central-1a.icewhale.io:11010",
    "udp://remote-us-east-2a.icewhale.io:11010",
    "udp://remote-ap-northeast-1a.icewhale.io:11010",
    "wss://et.icewhale.io/",
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BuiltinPeerList {
    peer: PeerConfig,
}

impl BuiltinPeerList {
    pub(crate) fn default_root() -> Self {
        Self {
            peer: PeerConfig {
                uri: BUILTIN_LIST_URI
                    .parse()
                    .expect("builtin discovery url must be valid"),
                peer_public_key: None,
            },
        }
    }

    pub(crate) fn try_from_peer(peer: PeerConfig) -> Option<Self> {
        (peer.uri.scheme() == "builtin").then_some(Self { peer })
    }
}

#[async_trait]
impl PeerList for BuiltinPeerList {
    fn peer(&self) -> &PeerConfig {
        &self.peer
    }

    async fn resolve(&self, _ctx: &ArcGlobalCtx) -> Result<Vec<PeerRef>, Error> {
        Ok(parse_peer_refs(&BUILTIN_PEER_REFS.join("\n")))
    }
}
