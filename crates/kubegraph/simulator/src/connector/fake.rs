use std::{
    collections::BTreeMap,
    mem::swap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use glob::glob;
use kubegraph_api::{
    graph::{NetworkEntry, NetworkEntrykey, NetworkNodeKey},
    provider::NetworkGraphProvider,
};
use kubegraph_parser::{Filter, FilterParser, Literal};
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
    constraints: BTreeMap<NetworkObjectMetadata, NetworkConstraint<Filter>>,
    functions: BTreeMap<NetworkObjectMetadata, NetworkFunction>,
    nodes: BTreeMap<NetworkObjectMetadata, NetworkNode>,
    nodes_new: Vec<(NetworkObjectMetadata, NetworkNode)>,
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
            NetworkObjectTemplate::Constraint(spec) => match spec.parse() {
                Ok(spec) => {
                    self.constraints.insert(metadata, spec);
                }
                Err(error) => {
                    warn!("Failed to parse constraint ({namespace}/{name}): {error}");
                }
            },
            NetworkObjectTemplate::Function(spec) => {
                self.functions.insert(metadata, spec);
            }
            NetworkObjectTemplate::Node(spec) => {
                self.nodes.insert(metadata.clone(), spec.clone());
                self.nodes_new.push((metadata, spec));
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
        // NOTE: ordered
        self.pull_nodes(graph).await?;
        // self.pull_edges(graph).await?;
        self.pull_constraints(graph).await?;
        self.pull_functions(graph).await?;
        Ok(())
    }
}

impl Connector {
    async fn pull_nodes(&mut self, graph: &impl NetworkGraphProvider) -> Result<()> {
        if self.nodes_new.is_empty() {
            return Ok(());
        }

        // unregister new nodes, taking the values to a local variable `nodes`
        let mut nodes = vec![];
        swap(&mut self.nodes_new, &mut nodes);

        let entries = nodes.into_iter().flat_map(|(key, value)| {
            let NetworkObjectMetadata { name, namespace } = key;
            let NetworkNode { values } = value;

            let entry_key = move |kind| NetworkNodeKey {
                kind,
                name: name.clone(),
                namespace: namespace.clone(),
            };

            values.into_iter().map(move |(kind, value)| NetworkEntry {
                key: NetworkEntrykey::Node(entry_key(kind)),
                value,
            })
        });

        graph.add_entries(entries).await
    }

    async fn pull_constraints(&mut self, graph: &impl NetworkGraphProvider) -> Result<()> {
        // TODO: to be implemented
        todo!()
    }

    async fn pull_functions(&mut self, graph: &impl NetworkGraphProvider) -> Result<()> {
        // TODO: to be implemented
        todo!()
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
                warn!("Skipping erroneous template file ({path:?}): {error}");
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
                warn!("Skipping erroneous template file ({path:?}): {error}");
                None
            }
        })
        .flatten())
}

trait NetworkComputer {
    type Output;

    fn compute(
        &self,
        graph: &impl NetworkGraphProvider,
    ) -> Result<<Self as NetworkComputer>::Output>;
}

impl NetworkComputer for NetworkConstraint<Filter> {
    type Output = bool;

    fn compute(
        &self,
        graph: &impl NetworkGraphProvider,
    ) -> Result<<Self as NetworkComputer>::Output> {
        todo!()
    }
}

trait NetworkParser {
    type Output;

    fn parse(&self) -> Result<<Self as NetworkParser>::Output>;
}

impl NetworkParser for NetworkConstraint {
    type Output = NetworkConstraint<Filter>;

    fn parse(&self) -> Result<<Self as NetworkParser>::Output> {
        let Self { filters, r#where } = self;

        let filter_parser = FilterParser::default();

        Ok(NetworkConstraint {
            filters: filters
                .iter()
                .map(|input| {
                    filter_parser
                        .parse(input)
                        .map_err(|error| anyhow!("{error}"))
                })
                .collect::<Result<_, _>>()?,
            r#where: r#where
                .iter()
                .map(|input| {
                    filter_parser
                        .parse(input)
                        .map_err(|error| anyhow!("{error}"))
                })
                .collect::<Result<_>>()?,
        })
    }
}
