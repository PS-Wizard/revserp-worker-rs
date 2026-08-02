use anyhow::Result;
use reqwest::Url;
use tokio::sync::Semaphore;

use crate::crawler::renderer::lightpanda::{LightPandaRenderedDocument, LightPandaSpawnConfig};

const MAX_LIGHTPANDA_INSTANCES: usize = 3;

pub struct RenderPool {
    permits: Semaphore,
    renderer_config: LightPandaSpawnConfig,
}

impl RenderPool {
    pub fn new(lp_config: LightPandaSpawnConfig) -> Self {
        RenderPool {
            permits: Semaphore::new(MAX_LIGHTPANDA_INSTANCES),
            renderer_config: lp_config,
        }
    }

    pub async fn render(&self, url: &Url) -> Result<LightPandaRenderedDocument> {
        let _permit = self.permits.acquire().await?;
        self.renderer_config.render(url).await
    }
}
