use std::{collections::BTreeMap, mem::swap, path::Path, time::Duration};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use kube::ResourceExt;
use kubegraph_api::{
    connector::{
        simulation::NetworkConnectorSimulationSpec, NetworkConnectorSourceRef,
        NetworkConnectorSpec, NetworkConnectors,
    },
    db::NetworkGraphDB,
    graph::{
        NetworkEdgeKey, NetworkEntry, NetworkEntryKey, NetworkGraphMetadata, NetworkNodeKey,
        NetworkValue,
    },
};
use polars::{
    frame::DataFrame,
    io::{csv::CsvReader, SerReader},
    series::Series,
};
use tokio::fs;
use tracing::{info, instrument, warn, Level};

#[derive(Default)]
pub struct NetworkConnector {
    db: Vec<NetworkConnectorItem>,

    values: BTreeMap<NetworkEntryKey, NetworkValue>,
    values_new: Vec<NetworkEntry>,
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
                .filter_map(|crd| {
                    let namespace = crd.namespace()?;
                    match crd.spec {
                        NetworkConnectorSpec::Simulation(spec) => {
                            Some(NetworkConnectorItem { namespace, spec })
                        }
                        _ => None,
                    }
                })
                .collect();

            for item in self.db.clone() {
                if let Err(error) = self.load_templates(&item).await {
                    let spec = &item.spec;
                    warn!("failed to load simulation templates {spec:?}: {error}");
                }
            }
        }
        if self.db.is_empty() {
            return Ok(());
        }

        self.pull_values(graph).await
    }
}

impl NetworkConnector {
    async fn pull_values(&mut self, graph: &impl NetworkGraphDB) -> Result<()> {
        if self.values_new.is_empty() {
            return Ok(());
        }

        // unregister new values, taking to a local variable `values`
        let mut values = vec![];
        swap(&mut self.values_new, &mut values);

        graph.add_entries(values).await
    }

    fn apply(&mut self, entry: NetworkEntry) {
        self.values.insert(entry.key.clone(), entry.value.clone());
        self.values_new.push(entry);
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn load_templates(&mut self, item: &NetworkConnectorItem) -> Result<()> {
        let NetworkConnectorItem {
            namespace,
            spec:
                NetworkConnectorSimulationSpec {
                    metadata,
                    path: base_dir,
                    key_edges,
                    key_nodes,
                },
        } = item;

        if let Some(df) = load_csv(base_dir, key_edges).await? {
            collect_edges(namespace, metadata, &df)?
                .into_iter()
                .for_each(|entry| self.apply(entry))
        }
        if let Some(df) = load_csv(base_dir, key_nodes).await? {
            collect_nodes(namespace, metadata, &df)?
                .into_iter()
                .for_each(|entry| self.apply(entry))
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct NetworkConnectorItem {
    namespace: String,
    spec: NetworkConnectorSimulationSpec,
}

async fn load_csv(base_dir: &Path, filename: &str) -> Result<Option<DataFrame>> {
    let mut path = base_dir.to_path_buf();
    path.push(filename);

    if fs::try_exists(&path).await? {
        CsvReader::from_path(&path)
            .map_err(
                |error| anyhow!("failed to load file {path}: {error}", path = path.display(),),
            )?
            .has_header(true)
            .finish()
            .map(Some)
            .map_err(|error| {
                anyhow!(
                    "failed to parse file {path}: {error}",
                    path = path.display(),
                )
            })
    } else {
        Ok(None)
    }
}

fn collect_edges<'a>(
    namespace: &'a str,
    metadata: &NetworkGraphMetadata,
    df: &'a DataFrame,
) -> Result<impl 'a + IntoIterator<Item = NetworkEntry>> {
    let NetworkGraphMetadata {
        capacity: key_capacity,
        flow: _,
        function: _,
        name: key_name,
        sink: key_sink,
        src: key_src,
        supply: _,
        unit_cost: key_unit_cost,
    } = metadata;

    // Get columns
    let name = validate_column(df, key_name)?;
    let capacity = validate_column(df, key_capacity)?;
    let sink = validate_column(df, key_sink)?;
    let src = validate_column(df, key_src)?;
    let unit_cost = validate_column(df, key_unit_cost)?;

    // Collect entries
    Ok(name
        .iter()
        .zip(capacity.iter())
        .zip(sink.iter())
        .zip(src.iter())
        .zip(unit_cost.iter())
        .filter_map(|((((name, capacity), sink), src), unit_cost)| {
            Some(NetworkEntry {
                key: NetworkEntryKey::Edge(NetworkEdgeKey {
                    interval_ms: None,
                    link: NetworkNodeKey {
                        name: name.to_string(),
                        namespace: namespace.into(),
                    },
                    sink: NetworkNodeKey {
                        name: sink.to_string(),
                        namespace: namespace.into(),
                    },
                    src: NetworkNodeKey {
                        name: src.to_string(),
                        namespace: namespace.into(),
                    },
                }),
                value: NetworkValue {
                    capacity: capacity.try_extract().ok(),
                    function: None,
                    flow: None,
                    supply: None,
                    unit_cost: unit_cost.try_extract().ok(),
                },
            })
        }))
}

fn collect_nodes<'a>(
    namespace: &'a str,
    metadata: &NetworkGraphMetadata,
    df: &'a DataFrame,
) -> Result<impl 'a + IntoIterator<Item = NetworkEntry>> {
    let NetworkGraphMetadata {
        capacity: key_capacity,
        flow: _,
        function: _,
        name: key_name,
        sink: _,
        src: _,
        supply: key_supply,
        unit_cost: key_unit_cost,
    } = metadata;

    // Get columns
    let name = validate_column(df, key_name)?;
    let capacity = validate_column(df, key_capacity)?;
    let supply = validate_column(df, key_supply)?;
    let unit_cost = validate_column(df, key_unit_cost)?;

    // Collect entries
    Ok(name
        .iter()
        .zip(capacity.iter())
        .zip(supply.iter())
        .zip(unit_cost.iter())
        .filter_map(|(((name, capacity), supply), unit_cost)| {
            Some(NetworkEntry {
                key: NetworkEntryKey::Node(NetworkNodeKey {
                    name: name.to_string(),
                    namespace: namespace.into(),
                }),
                value: NetworkValue {
                    capacity: capacity.try_extract().ok(),
                    function: None,
                    flow: None,
                    supply: supply.try_extract().ok(),
                    unit_cost: unit_cost.try_extract().ok(),
                },
            })
        }))
}

fn validate_column<'a>(df: &'a DataFrame, name: &str) -> Result<&'a Series> {
    let column = df.column(name)?;
    if column.is_empty() {
        bail!("empty column: {name}")
    }
    Ok(column)
}
