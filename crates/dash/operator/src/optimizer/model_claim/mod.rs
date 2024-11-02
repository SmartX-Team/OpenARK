mod db;
mod kubernetes;
mod object;

use anyhow::{bail, Result};
use async_trait::async_trait;
use byte_unit::Byte;
use dash_api::{
    model::ModelCrd,
    model_claim::ModelClaimBindingPolicy,
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy, ModelStorageBindingStorageKind,
        ModelStorageBindingStorageKindOwnedSpec,
    },
    storage::{
        ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec, StorageResourceRequirements,
    },
};
use dash_provider::storage::KubernetesStorageClient;
use dash_provider_api::data::Capacity;
use futures::{stream::FuturesUnordered, StreamExt};
use k8s_openapi::api::core::v1::ResourceRequirements;
use kube::{Client, ResourceExt};
use prometheus_http_query::Client as PrometheusClient;
use tracing::{instrument, warn, Level};

pub struct ModelClaimOptimizer<'namespace, 'kube> {
    binding_policy: ModelClaimBindingPolicy,
    field_manager: &'kube str,
    kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
    prometheus_client: &'kube PrometheusClient,
}

impl<'namespace, 'kube> ModelClaimOptimizer<'namespace, 'kube> {
    pub const fn new(
        field_manager: &'kube str,
        kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
        prometheus_client: &'kube PrometheusClient,
        binding_policy: ModelClaimBindingPolicy,
    ) -> Self {
        Self {
            binding_policy,
            field_manager,
            kubernetes_storage,
            prometheus_client,
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn optimize_model_storage_binding(
        &self,
        model: &ModelCrd,
        storage: Option<ModelStorageKind>,
        resources: Option<ResourceRequirements>,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<Option<ModelStorageBindingCrd>> {
        // Collect all storages
        let crs = self
            .kubernetes_storage
            .load_model_storages_by(|spec| {
                storage
                    .map(|storage| storage == spec.to_kind())
                    .unwrap_or(true)
            })
            .await?;

        // Collect all metrics
        let storages = crs
            .iter()
            .filter_map(|storage| {
                storage
                    .status
                    .as_ref()
                    .and_then(|status| status.kind.as_ref())
                    .cloned()
                    .map(|kind| async move {
                        let KubernetesStorageClient { namespace, kube } = self.kubernetes_storage;
                        let storage_name = storage.name_any();

                        Storage {
                            data: storage,
                            capacity: kind
                                .get_capacity(kube, namespace, model, &storage_name)
                                .await
                                .unwrap_or_else(|error| {
                                    warn!("failed to get capacity: {error}");
                                    None
                                }),
                            traffic: kind
                                .get_traffic(
                                    self.prometheus_client,
                                    namespace,
                                    model,
                                    &storage_name,
                                )
                                .await
                                .unwrap_or_else(|error| {
                                    warn!("failed to get capacity: {error}");
                                    TrafficMetrics::default()
                                }),
                        }
                    })
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter();

        // Filter by quota
        let quota = resources.quota();
        let affordable_storages = storages.filter(|storage| match (quota, storage.capacity) {
            (Some(quota), Some(capacity)) => quota <= capacity.available(),
            (Some(_), None) => false,
            (None, _) => true,
        });

        // TODO: optimize by given binding policy (ASAP)
        // TODO: scrap informantions (from prometheus?)
        let best_storage = match self.binding_policy {
            ModelClaimBindingPolicy::Balanced => {
                bail!("Unimplemented yet ({})!", self.binding_policy)
            }
            ModelClaimBindingPolicy::LowestCopy => match affordable_storages
                .filter(|storage| storage.capacity.is_some())
                .max_by_key(|storage| storage.capacity.unwrap().available())
                .map(|storage| storage.data)
            {
                Some(storage) => storage,
                None => return Ok(None),
            },
            // TODO: scrap latency informantions (from prometheus?)
            ModelClaimBindingPolicy::LowestLatency => {
                bail!("Unimplemented yet ({})!", self.binding_policy)
            }
        };

        let storage_binding =
            ModelStorageBindingStorageKind::Owned(ModelStorageBindingStorageKindOwnedSpec {
                target: best_storage.name_any(),
            });

        self.kubernetes_storage
            .create_model_storage_binding(
                self.field_manager,
                model.name_any(),
                storage_binding,
                resources,
                deletion_policy,
            )
            .await
            .map(Some)
    }
}

struct Storage<'a> {
    capacity: Option<Capacity>,
    data: &'a ModelStorageCrd,
    traffic: TrafficMetrics,
}

#[async_trait]
pub trait GetCapacity {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        _model: &ModelCrd,
        storage_name: &str,
    ) -> Result<Option<Capacity>> {
        self.get_capacity_global(kube, namespace, storage_name)
            .await
    }

    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: &str,
    ) -> Result<Option<Capacity>>;
}

#[async_trait]
impl GetCapacity for ModelStorageKindSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        model: &ModelCrd,
        storage_name: &str,
    ) -> Result<Option<Capacity>> {
        match self {
            ModelStorageKindSpec::Database(storage) => {
                storage
                    .get_capacity(kube, namespace, model, storage_name)
                    .await
            }
            ModelStorageKindSpec::Kubernetes(storage) => {
                storage
                    .get_capacity(kube, namespace, model, storage_name)
                    .await
            }
            ModelStorageKindSpec::ObjectStorage(storage) => {
                storage
                    .get_capacity(kube, namespace, model, storage_name)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: &str,
    ) -> Result<Option<Capacity>> {
        match self {
            ModelStorageKindSpec::Database(storage) => {
                storage
                    .get_capacity_global(kube, namespace, storage_name)
                    .await
            }
            ModelStorageKindSpec::Kubernetes(storage) => {
                storage
                    .get_capacity_global(kube, namespace, storage_name)
                    .await
            }
            ModelStorageKindSpec::ObjectStorage(storage) => {
                storage
                    .get_capacity_global(kube, namespace, storage_name)
                    .await
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct TrafficMetrics {
    pub global_bps: Option<Byte>,
    pub model_bps: Option<Byte>,
}

#[async_trait]
pub trait GetTraffic {
    async fn get_traffic<'namespace, 'kube>(
        &self,
        _prometheus_client: &'kube PrometheusClient,
        _namespace: &'namespace str,
        _model: &ModelCrd,
        _storage_name: &str,
    ) -> Result<TrafficMetrics> {
        Ok(TrafficMetrics::default())
    }
}

#[async_trait]
impl GetTraffic for ModelStorageKindSpec {
    async fn get_traffic<'namespace, 'kube>(
        &self,
        prometheus_client: &'kube PrometheusClient,
        namespace: &'namespace str,
        model: &ModelCrd,
        storage_name: &str,
    ) -> Result<TrafficMetrics> {
        match self {
            ModelStorageKindSpec::Database(storage) => {
                storage
                    .get_traffic(prometheus_client, namespace, model, storage_name)
                    .await
            }
            ModelStorageKindSpec::Kubernetes(storage) => {
                storage
                    .get_traffic(prometheus_client, namespace, model, storage_name)
                    .await
            }
            ModelStorageKindSpec::ObjectStorage(storage) => {
                storage
                    .get_traffic(prometheus_client, namespace, model, storage_name)
                    .await
            }
        }
    }
}
