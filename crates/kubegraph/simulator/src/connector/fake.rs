use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use glob::glob;
use kubegraph_api::provider::NetworkGraphProvider;
use kubegraph_simulator_schema::{NetworkObjectCrd, NetworkObjectMetadata, NetworkObjectTemplate};
use serde::Deserialize;
use tracing::{info, instrument, warn, Level};

#[derive(Parser)]
pub struct ConnectorArgs {
    #[clap(
        short = 'p',
        long,
        env = "KUBEGRAPH_SIMULATOR_BASE_DIR",
        default_value = "."
    )]
    base_dir: PathBuf,
}

#[derive(Default)]
pub struct Connector {
    templates: BTreeMap<NetworkObjectMetadata, NetworkObjectTemplate>,
}

impl Connector {
    pub fn try_new(args: &ConnectorArgs) -> Result<Self> {
        let ConnectorArgs { base_dir } = args;

        Ok(Self {
            templates: load_templates(base_dir)
                .map_err(|error| anyhow!("failed to load simulator templates: {error}"))?
                .into_iter()
                .map(
                    |NetworkObjectCrd {
                         api_version: _,
                         metadata,
                         template,
                     }| { (metadata, template) },
                )
                .collect(),
        })
    }
}

#[async_trait]
impl ::kubegraph_api::connector::Connector for Connector {
    #[inline]
    fn name(&self) -> &str {
        "fake"
    }

    #[inline]
    fn interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn pull(&mut self, graph: &impl NetworkGraphProvider) -> Result<()> {
        // TODO: to be implemented
        Ok(())
    }
}

fn load_templates(base_dir: &Path) -> Result<impl IntoIterator<Item = NetworkObjectCrd>> {
    let base_dir = base_dir.display();
    let entries = glob(&format!("{base_dir}/**/*.yaml"))
        .map_err(|error| anyhow!("failed to read glob pattern: {error}"))?;

    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(error) => {
                let path = error.path();
                let error = error.error();
                warn!("Skipping errorous template ({path:?}): {error}");
                None
            }
        })
        .filter_map(|path| match ::std::fs::read_to_string(&path) {
            Ok(raw) => {
                info!("Loading template: {path:?}");
                Some(
                    ::serde_yaml::Deserializer::from_str(&raw)
                        .into_iter()
                        .filter_map(
                            move |document| match NetworkObjectCrd::deserialize(document) {
                                Ok(item) => Some(item),
                                Err(error) => {
                                    warn!("Skipping parsing YAML template ({path:?}): {error}");
                                    None
                                }
                            },
                        )
                        .collect::<Vec<_>>(),
                )
            }
            Err(error) => {
                warn!("Skipping errorous template ({path:?}): {error}");
                None
            }
        })
        .flatten())
}
