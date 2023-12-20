use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use byte_unit::Byte;
use dash_api::{model_claim::ModelClaimBindingPolicy, storage::ModelStorageCrd};
use dash_collector_api::{
    metadata::ObjectMetadata,
    metrics::{edge::EdgeMetric, node::NodeMetric, MetricRow},
};
use dash_optimizer_fallback::GetCapacity;
use kube::{Client, ResourceExt};
use maplit::btreemap;
use tokio::sync::RwLock;
use tracing::{info, instrument, warn, Level};

use crate::{ctx::WorldContext, plan::Plan};

#[derive(Clone, Default)]
pub struct World {
    namespaces: BTreeMap<String, Arc<RwLock<Namespace>>>,
}

impl World {
    pub fn get(&self, namespace: &str) -> Option<Arc<RwLock<Namespace>>> {
        self.namespaces.get(namespace).cloned()
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    #[must_use]
    pub async fn add_storage(&mut self, crd: ModelStorageCrd) -> Result<Option<NamespacePlan>> {
        let namespace = crd
            .namespace()
            .ok_or_else(|| anyhow!("world namespace should be exist"))?;

        let storage = self.namespaces.entry(namespace.clone()).or_default();
        storage.write().await.add_storage(namespace, crd)
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn discover_storage<'a>(
        &self,
        kube: &Client,
        metadata: &ObjectMetadata<'a>,
    ) -> Result<()> {
        let ObjectMetadata { name, namespace } = metadata;
        let storage = match self.namespaces.get(namespace.as_ref()).cloned() {
            Some(storage) => storage,
            None => {
                warn!("storage namespace has been removed: {namespace}");
                return Ok(());
            }
        };
        let index = match storage.read().await.names.get(name.as_ref()).copied() {
            Some(index) => {
                info!("discovering storage: {metadata}");
                index
            }
            None => {
                warn!("storage has been removed: {metadata}");
                return Ok(());
            }
        };

        // TODO: estimate latency / bandwidth test
        // todo!();

        {
            let crd = match storage
                .read()
                .await
                .nodes
                .get(index)
                .map(|node| &node.crd)
                .cloned()
            {
                Some(crd) => crd,
                None => {
                    warn!("node has been removed: {metadata}");
                    return Ok(());
                }
            };
            if let Some(capacity) = crd
                .get_capacity_global(kube, namespace, name.to_string())
                .await?
            {
                let mut storage = storage.write().await;
                if let Some(node) = storage.nodes.get_mut(index) {
                    node.discovered = true;
                    node.capacity = parse_byte(capacity.capacity)?;
                    node.usage = parse_byte(capacity.usage)?;
                }
            }
        }

        Ok(())
    }

    pub async fn update_metrics<'a>(&mut self, metrics: Vec<MetricRow<'a>>) {
        for MetricRow {
            metadata: ObjectMetadata { name, namespace },
            value,
        } in metrics
        {
            let storage = self.namespaces.entry(namespace.to_string()).or_default();
            storage
                .write()
                .await
                .update_node_metric(name.as_ref(), value)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Namespace {
    names: BTreeMap<String, usize>,
    nodes: Vec<NodeStatus>,
}

impl Namespace {
    pub fn exists(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub fn is_ready(&self, name: &str) -> bool {
        self.names
            .get(name)
            .and_then(|index| self.nodes.get(*index).map(|node| node.discovered))
            .unwrap_or_default()
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    #[must_use]
    fn add_storage(
        &mut self,
        namespace: String,
        crd: ModelStorageCrd,
    ) -> Result<Option<NamespacePlan>> {
        let metadata = ObjectMetadata {
            name: crd.name_any().into(),
            namespace: namespace.into(),
        };

        match self.names.get(metadata.name.as_ref()).copied() {
            Some(index) => {
                if self.nodes[index].crd.metadata == crd.metadata {
                    Ok(None)
                } else {
                    // remove node
                    for node in &mut self.nodes {
                        node.edges.remove(&index);
                    }
                    self.nodes[index] = NodeStatus::new(crd, index);
                    Ok(Some(NamespacePlan::Discover { metadata }))
                }
            }
            None => {
                let index = self.names.len();

                self.names.insert(metadata.name.to_string(), index);
                self.nodes.push(NodeStatus::new(crd, index));
                Ok(Some(NamespacePlan::Discover { metadata }))
            }
        }
    }

    fn update_node_metric(&mut self, name: &str, value: NodeMetric) {
        if let Some(status) = self
            .names
            .get(name)
            .copied()
            .and_then(|id| self.nodes.get_mut(id))
        {
            status.metric = value;
        }
    }

    #[must_use]
    pub fn solve_next_model_storage_binding(
        &self,
        name: &str,
        policy: ModelClaimBindingPolicy,
    ) -> Option<String> {
        self.names
            .keys()
            .zip(self.nodes.iter().map(|node| node.available_quota()))
            .max_by_key(|(_, available)| *available)
            .map(|(name, _)| name.clone())
    }

    #[must_use]
    pub fn solve_next_storage(
        &self,
        name: &str,
        policy: ModelClaimBindingPolicy,
    ) -> Option<Arc<ModelStorageCrd>> {
        // TODO: to be implemented
        let get_score: fn(usize, usize) -> i64 = match policy {
            ModelClaimBindingPolicy::Balanced => |start, end| -((start + end) as i64),
            ModelClaimBindingPolicy::LowestCopy => |start, end| -((start + end) as i64),
            ModelClaimBindingPolicy::LowestLatency => |start, end| -((start + end) as i64),
        };
        let try_get_score = |start: usize, end: usize| {
            if self
                .nodes
                .get(start)
                .map(|node| node.edges.contains_key(&end))
                .unwrap_or_default()
            {
                get_score(start, end)
            } else {
                0
            }
        };

        let start = self.names.get(name).copied()?;
        (0..self.names.len())
            .filter(|&end| start != end)
            .filter_map(|index| self.nodes.get(index).map(|node| (index, node)))
            .filter(|(_, node)| node.discovered)
            .rev()
            .max_by_key(|(end, _)| try_get_score(start, *end))
            .map(|(_, node)| node.crd.clone())
    }
}

#[derive(Debug)]
pub enum NamespacePlan {
    Discover { metadata: ObjectMetadata<'static> },
}

#[async_trait]
impl Plan for NamespacePlan {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn exec(&self, ctx: &WorldContext) -> Result<()> {
        match self {
            Self::Discover { metadata } => {
                let ObjectMetadata { name, namespace } = metadata;
                let storage = match ctx
                    .data
                    .read()
                    .await
                    .namespaces
                    .get(namespace.as_ref())
                    .cloned()
                {
                    Some(namespace) => namespace,
                    None => {
                        warn!("world namespace has been removed: {namespace}");
                        return Ok(());
                    }
                };
                let index = match storage.read().await.names.get(name.as_ref()).copied() {
                    Some(index) => {
                        info!("discovering storage: {metadata}");
                        index
                    }
                    None => {
                        warn!("world namespace has been removed: {metadata}");
                        return Ok(());
                    }
                };

                // TODO: estimate latency / bandwidth test
                // todo!();

                {
                    let crd = match storage
                        .read()
                        .await
                        .nodes
                        .get(index)
                        .map(|node| &node.crd)
                        .cloned()
                    {
                        Some(crd) => crd,
                        None => {
                            warn!("node has been removed: {metadata}");
                            return Ok(());
                        }
                    };
                    if let Some(capacity) = crd
                        .get_capacity_global(&ctx.kube, namespace, name.to_string())
                        .await?
                    {
                        let mut storage = storage.write().await;
                        if let Some(node) = storage.nodes.get_mut(index) {
                            node.discovered = true;
                            node.capacity = parse_byte(capacity.capacity)?;
                            node.usage = parse_byte(capacity.usage)?;
                        }
                    }
                }

                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug)]
struct NodeStatus {
    capacity: i64,
    crd: Arc<ModelStorageCrd>,
    discovered: bool,
    edges: BTreeMap<usize, EdgeMetric>,
    metric: NodeMetric,
    usage: i64,
}

impl NodeStatus {
    fn new(crd: impl Into<Arc<ModelStorageCrd>>, index: usize) -> Self {
        Self {
            capacity: 0,
            crd: crd.into(),
            discovered: false,
            edges: btreemap!(
                index => EdgeMetric::default(),
            ),
            metric: NodeMetric::default(),
            usage: 0,
        }
    }

    fn available_quota(&self) -> i64 {
        self.capacity
            .checked_sub(self.usage)
            .unwrap_or_default()
            .max(0)
    }
}

fn parse_byte(byte: Byte) -> Result<i64> {
    byte.get_bytes()
        .try_into()
        .map_err(|error| anyhow!("failed to parse capacity byte: {error}"))
}
