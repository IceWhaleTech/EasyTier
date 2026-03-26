use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use http_req::request::{RedirectPolicy, Request};

use crate::{
    common::{config::PeerConfig, error::Error, global_ctx::ArcGlobalCtx},
    VERSION,
};

use super::{parse_peer_refs, peer_list::PeerList, PeerRef};

const HTTP_LIST_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HttpPeerList {
    peer: PeerConfig,
    target: url::Url,
}

impl HttpPeerList {
    pub(crate) fn try_from_peer(peer: PeerConfig) -> Result<Option<Self>, Error> {
        match peer.uri.scheme() {
            "peerlist+http" | "peerlist+https" => {
                let target = peer
                    .uri
                    .as_str()
                    .replacen("peerlist+http://", "http://", 1)
                    .replacen("peerlist+https://", "https://", 1)
                    .parse::<url::Url>()
                    .map_err(|_| {
                        Error::InvalidUrl(format!("invalid peer list http url: {}", peer.uri))
                    })?;
                Ok(Some(Self { peer, target }))
            }
            _ => Ok(None),
        }
    }
}

#[async_trait]
impl PeerList for HttpPeerList {
    fn peer(&self) -> &PeerConfig {
        &self.peer
    }

    fn refresh_interval(&self) -> Option<Duration> {
        Some(HTTP_LIST_REFRESH_INTERVAL)
    }

    async fn resolve(&self, ctx: &ArcGlobalCtx) -> Result<Vec<PeerRef>, Error> {
        let body = fetch_http_body(self.target.clone(), ctx.get_network_name()).await?;
        Ok(parse_peer_refs(&body))
    }
}

async fn fetch_http_body(url: url::Url, network_name: String) -> Result<String, Error> {
    let url_for_error = url.clone();
    let user_agent = format!("easytier/{}", VERSION);
    let body = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, Error> {
        let uri = http_req::uri::Uri::try_from(url.as_str())
            .with_context(|| format!("parsing peer list http url failed: {}", url))?;
        let mut body = Vec::new();

        let response = Request::new(&uri)
            .header("User-Agent", &user_agent)
            .header("X-Network-Name", &network_name)
            .redirect_policy(RedirectPolicy::Limit(5))
            .timeout(Duration::from_secs(20))
            .send(&mut body)
            .with_context(|| format!("fetching peer list http failed: {}", uri))?;

        if !response.status_code().is_success() {
            return Err(Error::InvalidUrl(format!(
                "unexpected peer list response: status {}, url {}",
                response.status_code(),
                url_for_error
            )));
        }

        Ok(body)
    })
    .await
    .map_err(|err| Error::AnyhowError(anyhow::anyhow!("peer list task join error: {}", err)))??;

    Ok(String::from_utf8_lossy(&body).to_string())
}
