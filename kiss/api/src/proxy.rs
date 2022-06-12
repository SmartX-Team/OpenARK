use std::path::PathBuf;

use ipis::{
    core::anyhow::{anyhow, bail, Result},
    env,
    tokio::fs,
};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProxyConfig {
    sites: Vec<ProxySite>,
}

impl ProxyConfig {
    pub async fn load() -> Result<Self> {
        let path: PathBuf = env::infer("KISS_PROXY_CONFIG_PATH")?;

        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => {
                serde_json::from_str(&fs::read_to_string(path).await?).map_err(Into::into)
            }
            Some("yaml") => {
                serde_yaml::from_str(&fs::read_to_string(path).await?).map_err(Into::into)
            }
            Some(_) => bail!("unsupported extension: {path:?}"),
            None => bail!("cannot infer the extension: {path:?}"),
        }
    }

    pub fn search(&self, site: &str, path: &str) -> Result<String> {
        self.sites
            .iter()
            .find(|s| s.name == site)
            .map(|s| format!("{}{path}", &s.host))
            .ok_or_else(|| anyhow!("failed to find the site: {site}"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProxySite {
    pub name: String,
    #[serde(with = "url_serde")]
    pub host: Url,
}
