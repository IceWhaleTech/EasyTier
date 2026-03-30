use async_trait::async_trait;

use crate::common::{config::PeerConfig, error::Error, global_ctx::ArcGlobalCtx};

use super::{parse_peer_refs, peer_list::PeerList, PeerRef};

const BUILTIN_LIST_URI: &str = "builtin://default";
const BUILTIN_PEER_REFS: &[&str] = &[
    // "tcp://root.remote-eu-central-1a.icewhale.io:11010",
    // "wss://root.remote-eu-central-1a.icewhale.io:11012",
    // "wss://root.remote-us-east-2a.icewhale.io:11012",
    // "tcp://root.remote-us-east-2a.icewhale.io:11010",
    // "tcp://root.remote-ap-northeast-1a.icewhale.io:11010",
    // "wss://root.remote-ap-northeast-1a.icewhale.io:11012",
    // "udp://root.remote-eu-central-1a.icewhale.io:11010",
    // "udp://root.remote-us-east-2a.icewhale.io:11010",
    // "udp://root.remote-ap-northeast-1a.icewhale.io:11010",
    // "tcp://root.remote-ap-southeast-1a.icewhale.io:11010",
    // "udp://root.remote-ap-southeast-1a.icewhale.io:11010",
    // "wss://root.remote-ap-southeast-1a.icewhale.io:11012",
    "wss://et.icewhale.io/",
    // "peerlist+https://raw.githubusercontent.com/IceWhaleTech/EasyTier/provider/peerlists/builtin-peers.txt"
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
