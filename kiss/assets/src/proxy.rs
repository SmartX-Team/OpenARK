use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use ark_core::env;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{instrument, Level};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    sites: Vec<ProxySite>,
}

impl ProxyConfig {
    #[instrument(level = Level::INFO, err(Display))]
    pub async fn load() -> Result<Self> {
        let path: PathBuf = env::infer("KISS_ASSETS_CONFIG_PATH")?;

        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => {
                ::serde_json::from_str(&fs::read_to_string(path).await?).map_err(Into::into)
            }
            Some("yaml") => {
                ::serde_yaml::from_str(&fs::read_to_string(path).await?).map_err(Into::into)
            }
            Some(_) => bail!("unsupported extension: {path:?}"),
            None => bail!("cannot infer the extension: {path:?}"),
        }
    }

    pub fn search(&self, site: &str, path: &str, query: &str) -> Result<String> {
        self.sites
            .iter()
            .find(|s| s.name == site)
            .map(|s| {
                let host = &s.host;
                if query.is_empty() {
                    format!("{host}{path}")
                } else {
                    format!("{host}{path}?{query}")
                }
            })
            .ok_or_else(|| anyhow!("failed to find the site: {site}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxySite {
    pub name: String,
    pub host: Url,
}
