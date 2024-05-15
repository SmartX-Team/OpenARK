mod schema;

use std::{collections::BTreeMap, mem::swap, time::Duration};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kubegraph_api::{
    connector::{
        NetworkConnectorSimulationSpec, NetworkConnectorSourceRef, NetworkConnectorSpec,
        NetworkConnectors,
    },
    db::NetworkGraphDB,
    graph::{NetworkEntry, NetworkEntryKey, NetworkNodeKey},
};
use kubegraph_parser::{Filter, FilterParser};
use serde::Deserialize;
use tokio::fs;
use tracing::{info, instrument, warn, Level};

use crate::schema::{
    constraint::NetworkConstraint, function::NetworkFunction, node::NetworkNode, NetworkObjectCrd,
    NetworkObjectMetadata, NetworkObjectTemplate,
};

#[derive(Default)]
pub struct NetworkConnector {
    db: Vec<NetworkConnectorSimulationSpec>,

    constraints: BTreeMap<NetworkObjectMetadata, NetworkConstraint<Filter>>,
    functions: BTreeMap<NetworkObjectMetadata, NetworkFunction>,
    nodes: BTreeMap<NetworkObjectMetadata, NetworkNode>,
    nodes_new: Vec<(NetworkObjectMetadata, NetworkNode)>,
}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnector for NetworkConnector {
    #[inline]
    fn name(&self) -> &str {
        "simulation"
    }

    #[inline]
    fn interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn pull(&mut self, graph: &(impl NetworkConnectors + NetworkGraphDB)) -> Result<()> {
        // update db
        if let Some(db) = graph
            .get_connectors(NetworkConnectorSourceRef::Simulation)
            .await
        {
            info!("Reloading simulation connector...");
            self.db = db
                .into_iter()
                .filter_map(|spec| match spec {
                    NetworkConnectorSpec::Simulation(spec) => Some(spec),
                    #[allow(unused_variables)]
                    _ => None,
                })
                .collect();

            for spec in self.db.clone() {
                if let Err(error) = self.load_templates(&spec).await {
                    warn!("failed to load simulation templates {spec:?}: {error}");
                }
            }
        }
        if self.db.is_empty() {
            return Ok(());
        }

        // NOTE: ordered
        self.pull_nodes(graph).await?;
        // self.pull_edges(graph).await?;
        self.pull_constraints(graph).await?;
        self.pull_functions(graph).await?;
        Ok(())
    }
}

impl NetworkConnector {
    async fn pull_nodes(&mut self, graph: &impl NetworkGraphDB) -> Result<()> {
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
                key: NetworkEntryKey::Node(entry_key(kind)),
                value,
            })
        });

        graph.add_entries(entries).await
    }

    async fn pull_constraints(&mut self, graph: &impl NetworkGraphDB) -> Result<()> {
        // TODO: to be implemented
        Ok(())
    }

    async fn pull_functions(&mut self, graph: &impl NetworkGraphDB) -> Result<()> {
        // TODO: to be implemented
        Ok(())
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

    #[instrument(level = Level::INFO, skip_all)]
    async fn load_templates(&mut self, spec: &NetworkConnectorSimulationSpec) -> Result<()> {
        let NetworkConnectorSimulationSpec { path } = spec;

        let mut file_entries = fs::read_dir(path).await.map_err(|error| {
            anyhow!(
                "failed to read directory {path}: {error}",
                path = path.display(),
            )
        })?;

        while let Some(entry) = file_entries.next_entry().await.map_err(|error| {
            anyhow!(
                "failed to read directory entry {path}: {error}",
                path = path.display(),
            )
        })? {
            let path = entry.path();
            match fs::read_to_string(&path).await {
                Ok(raw) => {
                    info!("Loading template file: {path:?}");
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
                        .for_each(|crd| self.apply(crd))
                }
                Err(error) => {
                    warn!("Skipping erroneous template file ({path:?}): {error}");
                }
            }
        }
        Ok(())
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
