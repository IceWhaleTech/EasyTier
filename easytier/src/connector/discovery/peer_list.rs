use std::time::Duration;

use async_trait::async_trait;

use crate::common::{config::PeerConfig, error::Error, global_ctx::ArcGlobalCtx};

use super::PeerRef;

#[async_trait]
pub(crate) trait PeerList: std::fmt::Debug + Send + Sync {
    fn peer(&self) -> &PeerConfig;

    fn refresh_interval(&self) -> Option<Duration> {
        None
    }

    async fn resolve(&self, ctx: &ArcGlobalCtx) -> Result<Vec<PeerRef>, Error>;
}
