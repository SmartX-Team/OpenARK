mod db;
mod kubernetes;
mod object;

use anyhow::Result;
use async_trait::async_trait;
use dash_api::{
    model::ModelCrd,
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingStorageKind,
        ModelStorageBindingStorageKindOwnedSpec,
    },
    storage::{ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec},
};
use dash_provider::storage::KubernetesStorageClient;
use dash_provider_api::data::Capacity;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::{Client, ResourceExt};
use tracing::{instrument, warn, Level};

pub struct ModelClaimOptimizer<'namespace, 'kube> {
    kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> ModelClaimOptimizer<'namespace, 'kube> {
    pub fn new(kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>) -> Self {
        Self { kubernetes_storage }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn optimize_model_storage_binding(
        &self,
        field_manager: &str,
        model: &ModelCrd,
        storage: Option<ModelStorageKind>,
    ) -> Result<Option<ModelStorageBindingCrd>> {
        let storages = self
            .kubernetes_storage
            .load_model_storages_by(|spec| {
                storage
                    .map(|storage| storage == spec.to_kind())
                    .unwrap_or(true)
            })
            .await?;

        let storage_sizes = storages
            .into_iter()
            .filter_map(|storage| {
                storage
                    .status
                    .as_ref()
                    .and_then(|status| status.kind.as_ref())
                    .cloned()
                    .map(|kind| async move {
                        let KubernetesStorageClient { namespace, kube } = self.kubernetes_storage;
                        let storage_name = storage.name_any();
                        let capacity = kind
                            .get_capacity(kube, namespace, model, storage_name)
                            .await
                            .unwrap_or_else(|error| {
                                warn!("failed to get capacity: {error}");
                                None
                            });

                        Some((storage, capacity?))
                    })
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await;

        let best_storage = match storage_sizes
            .into_iter()
            .flatten()
            .max_by_key(|(_, capacity)| capacity.available())
            .map(|(storage, _)| storage)
        {
            Some(storage) => storage,
            None => return Ok(None),
        };

        let storage_binding =
            ModelStorageBindingStorageKind::Owned(ModelStorageBindingStorageKindOwnedSpec {
                target: best_storage.name_any(),
            });

        self.kubernetes_storage
            .create_model_storage_binding(field_manager, model.name_any(), storage_binding)
            .await
            .map(Some)
    }
}

#[async_trait]
pub trait GetCapacity {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        _model: &ModelCrd,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        self.get_capacity_global(kube, namespace, storage_name)
            .await
    }

    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: String,
    ) -> Result<Option<Capacity>>;
}

#[async_trait]
impl GetCapacity for ModelStorageCrd {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        model: &ModelCrd,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        match self.status.as_ref().and_then(|status| status.kind.as_ref()) {
            Some(kind) => {
                kind.get_capacity(kube, namespace, model, storage_name)
                    .await
            }
            None => Ok(None),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity_global<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        storage_name: String,
    ) -> Result<Option<Capacity>> {
        match self.status.as_ref().and_then(|status| status.kind.as_ref()) {
            Some(kind) => {
                kind.get_capacity_global(kube, namespace, storage_name)
                    .await
            }
            None => Ok(None),
        }
    }
}

#[async_trait]
impl GetCapacity for ModelStorageKindSpec {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn get_capacity<'namespace, 'kube>(
        &self,
        kube: &'kube Client,
        namespace: &'namespace str,
        model: &ModelCrd,
        storage_name: String,
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
        storage_name: String,
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
