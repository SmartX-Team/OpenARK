use std::{collections::HashMap, fmt, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use byte_unit::Byte;
use dash_api::{model_claim::ModelClaimBindingPolicy, storage::ModelStorageCrd};
use dash_optimizer_fallback::GetCapacity;
use kube::{api::ListParams, Api, Client, ResourceExt};
use ndarray::{Array0, Array1, Array2, Axis, DataMut, RawData, ViewRepr};
use tokio::sync::RwLock;
use tracing::{info, instrument, warn, Level};

use crate::{ctx::OptimizerContext, plan::Plan};

pub struct StorageLoader<'a> {
    ctx: &'a OptimizerContext,
}

impl<'a> StorageLoader<'a> {
    pub fn new(ctx: &'a OptimizerContext) -> Self {
        Self { ctx }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn load(&self) -> Result<()> {
        let kube = &*self.ctx.kube;
        let api = Api::<ModelStorageCrd>::all(kube.clone());
        let lp = ListParams::default();
        let crds = api.list(&lp).await?.items;

        let mut plans = Vec::with_capacity(crds.len());
        {
            let mut storage = self.ctx.storage.write().await;
            for crd in crds {
                if let Some(plan) = storage.add_storage(kube, crd).await? {
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

#[derive(Clone, Debug, Default)]
pub struct StorageDimension {
    dimensions: HashMap<String, Arc<RwLock<NamespacedStorageDimension>>>,
}

impl StorageDimension {
    pub fn get(&self, namespace: &str) -> Option<Arc<RwLock<NamespacedStorageDimension>>> {
        self.dimensions.get(namespace).cloned()
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    #[must_use]
    pub async fn add_storage(
        &mut self,
        kube: &Client,
        crd: ModelStorageCrd,
    ) -> Result<Option<StoragePlan>> {
        let namespace = crd
            .namespace()
            .ok_or_else(|| anyhow!("storage namespace should be exist"))?;

        let storage = self.dimensions.entry(namespace.clone()).or_default();
        storage.write().await.add_storage(kube, namespace, crd)
    }
}

#[derive(Clone, Debug, Default)]
pub struct NamespacedStorageDimension {
    capacity: Array1<i64>,
    crds: Vec<Arc<ModelStorageCrd>>,
    latency_ms: Array2<i64>,
    map: Array2<bool>,
    names: HashMap<String, usize>,
    throughput_per_sec: Array2<i64>,
    usage: Array1<i64>,
}

impl NamespacedStorageDimension {
    pub fn is_ready(&self, name: &str) -> bool {
        self.names
            .get(name)
            .copied()
            .and_then(|index| self.map.get((index, index)).copied())
            .unwrap_or_default()
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    #[must_use]
    fn add_storage(
        &mut self,
        kube: &Client,
        namespace: String,
        crd: ModelStorageCrd,
    ) -> Result<Option<StoragePlan>> {
        let metadata = StorageMetadata {
            name: crd.name_any(),
            namespace,
        };

        match self.names.get(&metadata.name).copied() {
            Some(index) => {
                if self.crds[index].metadata == crd.metadata {
                    Ok(None)
                } else {
                    set_vector_default(&mut self.capacity, index);
                    self.crds[index] = Arc::new(crd);
                    set_matrix_default(&mut self.latency_ms, index);
                    set_matrix_default(&mut self.map, index);
                    /* skip changing names index */
                    set_matrix_default(&mut self.throughput_per_sec, index);
                    set_vector_default(&mut self.usage, index);
                    Ok(Some(StoragePlan::Discover { metadata }))
                }
            }
            None => {
                let index = self.names.len();

                grow_vector_default(&mut self.capacity)?;
                self.crds.push(Arc::new(crd));
                grow_matrix_default(&mut self.latency_ms)?;
                grow_matrix_default(&mut self.map)?;
                self.names.insert(metadata.name.clone(), index);
                grow_matrix_default(&mut self.throughput_per_sec)?;
                grow_vector_default(&mut self.usage)?;
                Ok(Some(StoragePlan::Discover { metadata }))
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    #[must_use]
    pub fn solve_next(
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

        let start = self.names.get(name).copied()?;
        let next = (0..self.names.len())
            .filter(|&end| start != end)
            .filter(|&index| self.map[(index, index)])
            .max_by_key(|&end| get_score(start, end))?;
        self.crds.get(next).cloned()
    }
}

#[derive(Debug)]
pub enum StoragePlan {
    Discover { metadata: StorageMetadata },
}

#[async_trait]
impl Plan for StoragePlan {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn exec(&self, ctx: &OptimizerContext) -> Result<()> {
        match self {
            Self::Discover { metadata } => {
                let StorageMetadata { name, namespace } = metadata;
                let storage = match ctx.storage.read().await.dimensions.get(namespace).cloned() {
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
                    let crd = storage.read().await.crds[index].clone();
                    if let Some(capacity) = crd
                        .get_capacity_global(&ctx.kube, namespace, name.clone())
                        .await?
                    {
                        let mut storage = storage.write().await;
                        storage.capacity[index] = parse_byte(capacity.capacity)?;
                        storage.map[(index, index)] = true;
                        storage.usage[index] = parse_byte(capacity.usage)?;
                    }
                }

                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StorageMetadata {
    name: String,
    namespace: String,
}

impl fmt::Display for StorageMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { name, namespace } = self;
        write!(f, "{namespace}/{name}")
    }
}

fn parse_byte(byte: Byte) -> Result<i64> {
    byte.get_bytes()
        .try_into()
        .map_err(|error| anyhow!("failed to parse capacity byte: {error}"))
}

fn grow_vector_default<T>(vector: &mut Array1<T>) -> Result<()>
where
    T: Copy + Default,
{
    grow_vector(vector, <T as Default>::default())
}

fn grow_vector<T>(vector: &mut Array1<T>, value: T) -> Result<()>
where
    T: Copy,
{
    let axis = Axis(0);
    let elem = Array0::from_elem((), value);

    vector.push(axis, elem.view())?;
    Ok(())
}

fn grow_matrix_default<T>(matrix: &mut Array2<T>) -> Result<()>
where
    T: Copy + Default,
{
    grow_matrix(matrix, <T as Default>::default())
}

fn grow_matrix<T>(matrix: &mut Array2<T>, value: T) -> Result<()>
where
    T: Copy,
{
    let axis = Axis(0);
    let len = matrix.len_of(axis) + 1;
    let vector = Array1::from_elem((len,), value);

    matrix.push_row(vector.slice_axis(axis, (0..(len - 1)).into()))?;
    matrix.push_column(vector.view())?;
    Ok(())
}

fn set_vector_default<T>(vector: &mut Array1<T>, index: usize)
where
    T: Default,
    for<'v> ViewRepr<&'v mut T>: DataMut + RawData<Elem = T>,
{
    set_vector(vector, index, <T as Default>::default())
}

fn set_vector<T>(vector: &mut Array1<T>, index: usize, value: T)
where
    for<'v> ViewRepr<&'v mut T>: DataMut + RawData<Elem = T>,
{
    vector[index] = value;
}

fn set_matrix_default<T>(matrix: &mut Array2<T>, index: usize)
where
    T: Copy + Default,
    for<'v> ViewRepr<&'v mut T>: DataMut + RawData<Elem = T>,
{
    set_matrix(matrix, index, <T as Default>::default())
}

fn set_matrix<T>(matrix: &mut Array2<T>, index: usize, value: T)
where
    T: Copy,
    for<'v> ViewRepr<&'v mut T>: DataMut + RawData<Elem = T>,
{
    matrix.index_axis_mut(Axis(0), index).fill(value);
    matrix.index_axis_mut(Axis(1), index).fill(value);
}
