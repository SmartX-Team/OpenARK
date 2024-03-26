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
use kubegraph_simulator_schema::{
    constraint::NetworkConstraint, function::NetworkFunction, node::NetworkNode, NetworkObjectCrd,
    NetworkObjectMetadata, NetworkObjectTemplate,
};
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
    constraints: BTreeMap<NetworkObjectMetadata, NetworkConstraint>,
    functions: BTreeMap<NetworkObjectMetadata, NetworkFunction>,
    nodes: BTreeMap<NetworkObjectMetadata, NetworkNode>,
}

impl Connector {
    pub fn try_new(args: &ConnectorArgs) -> Result<Self> {
        let ConnectorArgs { base_dir } = args;
        let mut connector = Self::default();

        load_templates(base_dir)
            .map_err(|error| anyhow!("failed to load simulator templates: {error}"))?
            .into_iter()
            .for_each(|crd| connector.apply(crd));
        Ok(connector)
    }

    fn apply(&mut self, crd: NetworkObjectCrd) {
        let NetworkObjectCrd {
            api_version,
            metadata: NetworkObjectMetadata { name, namespace },
            template: _,
        } = &crd;

        match api_version.as_str() {
            "kubegraph.ulagbulag.io/v1alpha1" => self.apply_unchecked(crd),
            api_version => warn!("Unsupported API version {api_version:?}: {namespace}/{name}"),
        }
    }

    fn apply_unchecked(&mut self, crd: NetworkObjectCrd) {
        let NetworkObjectCrd {
            api_version: _,
            metadata,
            template,
        } = crd;

        let NetworkObjectMetadata { name, namespace } = &metadata;
        let r#type = template.name();
        info!("Applying {type} connector: {namespace}/{name}");

        match template {
            NetworkObjectTemplate::Constraint(spec) => {
                self.constraints.insert(metadata, spec);
            }
            NetworkObjectTemplate::Function(spec) => {
                self.functions.insert(metadata, spec);
            }
            NetworkObjectTemplate::Node(spec) => {
                self.nodes.insert(metadata, spec);
            }
        }
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
                warn!("Skipping errorous template file ({path:?}): {error}");
                None
            }
        })
        .filter_map(|path| match ::std::fs::read_to_string(&path) {
            Ok(raw) => {
                info!("Loading template file: {path:?}");
                Some(
                    ::serde_yaml::Deserializer::from_str(&raw)
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
                warn!("Skipping errorous template file ({path:?}): {error}");
                None
            }
        })
        .flatten())
}
