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
    storage::{ModelStorageKind, ModelStorageKindSpec},
};
use dash_provider::storage::KubernetesStorageClient;
use dash_provider_api::data::Capacity;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::ResourceExt;
use tracing::{instrument, warn, Level};

pub struct OptimizerClient<'namespace, 'kube> {
    kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> OptimizerClient<'namespace, 'kube> {
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
                        let storage_name = storage.name_any();
                        let capacity = match &kind {
                            ModelStorageKindSpec::Database(spec) => {
                                spec.get_capacity(self.kubernetes_storage, model, storage_name)
                                    .await
                            }
                            ModelStorageKindSpec::Kubernetes(spec) => {
                                spec.get_capacity(self.kubernetes_storage, model, storage_name)
                                    .await
                            }
                            ModelStorageKindSpec::ObjectStorage(spec) => {
                                spec.get_capacity(self.kubernetes_storage, model, storage_name)
                                    .await
                            }
                        }
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
trait GetCapacity {
    async fn get_capacity<'namespace, 'kind>(
        &self,
        kubernetes_storage: KubernetesStorageClient<'namespace, 'kind>,
        model: &ModelCrd,
        storage_name: String,
    ) -> Result<Option<Capacity>>;
}
