use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use byte_unit::Byte;
use dash_api::{model_claim::ModelClaimBindingPolicy, storage::ModelStorageCrd};
use dash_optimizer_api::{optimize, ObjectMetadata};
use dash_optimizer_fallback::GetCapacity;
use dash_pipe_provider::{PipeArgs, PipeMessage, RemoteFunction};
use futures::FutureExt;
use itertools::Itertools;
use kube::{api::ListParams, Api, ResourceExt};
use maplit::btreemap;
use tokio::sync::RwLock;
use tracing::{info, instrument, warn, Level};

use crate::{
    ctx::{OptimizerContext, Timeout},
    plan::Plan,
};

#[derive(Clone)]
pub struct Optimizer {
    ctx: OptimizerContext,
}

#[async_trait]
impl crate::ctx::OptimizerService for Optimizer {
    fn new(ctx: &OptimizerContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn loop_forever(self) -> Result<()> {
        info!("creating messenger: storage optimizer");

        let pipe = PipeArgs::with_function(self)?
            .with_ignore_sigint(true)
            .with_model_in(Some(optimize::storage::model_in()?))
            .with_model_out(Some(optimize::storage::model_out()?));
        pipe.loop_forever_async().await
    }
}

#[async_trait]
impl RemoteFunction for Optimizer {
    type Input = optimize::storage::Request;
    type Output = optimize::storage::Response;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output, ()>> {
        let optimize::storage::Request {
            policy,
            storage: ObjectMetadata { name, namespace },
        } = &input.value;

        match self
            .ctx
            .get(namespace, name, Timeout::Unlimited)
            .then(|option| async {
                match option {
                    Some(namespace) => namespace.read().await.solve_next_storage(name, *policy),
                    None => None,
                }
            })
            .await
        {
            Some(target) => {
                let value = target.name_any().clone();
                Ok(PipeMessage::with_request(&input, vec![], Some(value)))
            }
            None => Ok(PipeMessage::with_request(&input, vec![], None)),
        }
    }
}

pub struct StorageLoader<'a> {
    ctx: &'a OptimizerContext,
}

impl<'a> StorageLoader<'a> {
    pub fn new(ctx: &'a OptimizerContext) -> Self {
        Self { ctx }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn load(&self) -> Result<()> {
        info!("loading storage info");
        let kube = &*self.ctx.kube;
        let api = Api::<ModelStorageCrd>::all(kube.clone());
        let lp = ListParams::default();
        let crds = api.list(&lp).await?.items;

        let mut plans = Vec::with_capacity(crds.len());
        {
            let mut storage = self.ctx.world.write().await;
            for crd in crds
                .into_iter()
                .sorted_by_key(|crd| crd.creation_timestamp())
            {
                if let Some(plan) = storage.add_storage(crd).await? {
                    plans.push(plan);
                }
            }
        }

        for plan in plans {
            self.ctx.add_plan(plan).await?;
        }
        Ok(())
    }
}

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
            .ok_or_else(|| anyhow!("storage namespace should be exist"))?;

        let storage = self.namespaces.entry(namespace.clone()).or_default();
        storage.write().await.add_storage(namespace, crd)
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
            name: crd.name_any(),
            namespace,
        };

        match self.names.get(&metadata.name).copied() {
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

                self.names.insert(metadata.name.clone(), index);
                self.nodes.push(NodeStatus::new(crd, index));
                Ok(Some(NamespacePlan::Discover { metadata }))
            }
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
    Discover { metadata: ObjectMetadata },
}

#[async_trait]
impl Plan for NamespacePlan {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn exec(&self, ctx: &OptimizerContext) -> Result<()> {
        match self {
            Self::Discover { metadata } => {
                let ObjectMetadata { name, namespace } = metadata;
                let storage = match ctx.world.read().await.namespaces.get(namespace).cloned() {
                    Some(storage) => storage,
                    None => {
                        warn!("storage namespace has been removed: {namespace}");
                        return Ok(());
                    }
                };
                let index = match storage.read().await.names.get(name).copied() {
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
                        .get_capacity_global(&ctx.kube, namespace, name.clone())
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

#[derive(Clone, Debug, Default)]
pub struct NodeMetric {
    pub elapsed_ns: i64,
    pub len: i64,
    pub total_bytes: i64,
}

#[derive(Clone, Debug, Default)]
pub struct EdgeMetric {
    pub latency_ms: i64,
    pub throughput_per_sec: i64,
}

fn parse_byte(byte: Byte) -> Result<i64> {
    byte.get_bytes()
        .try_into()
        .map_err(|error| anyhow!("failed to parse capacity byte: {error}"))
}
