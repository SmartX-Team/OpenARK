use anyhow::{anyhow, bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy, ModelStorageBindingStorageSpec,
    },
    storage::{
        db::ModelStorageDatabaseSpec, kubernetes::ModelStorageKubernetesSpec,
        object::ModelStorageObjectSpec, ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec,
        ModelStorageSpec, StorageResourceRequirements,
    },
};
use dash_provider::storage::{
    assert_source_is_none, assert_source_is_same, DatabaseStorageClient, KubernetesStorageClient,
    ObjectStorageClient,
};
use futures::TryFutureExt;
use itertools::Itertools;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::{api::ObjectMeta, Resource, ResourceExt};
use tracing::{instrument, Level};

pub struct ModelStorageValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
    pub prometheus_url: &'kube str,
}

impl<'namespace, 'kube> ModelStorageValidator<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn validate_model_storage(
        &self,
        name: &str,
        metadata: &ObjectMeta,
        spec: &ModelStorageSpec,
    ) -> Result<Option<u128>> {
        if spec.kind.is_unique() {
            self.validate_model_storage_conflict(name, spec.kind.to_kind())
                .await?;
        }

        match &spec.kind {
            ModelStorageKindSpec::Database(spec) => {
                self.validate_model_storage_database(spec).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => self.validate_model_storage_kubernetes(spec),
            ModelStorageKindSpec::ObjectStorage(spec) => {
                self.validate_model_storage_object(name, metadata, spec)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_conflict(
        &self,
        name: &str,
        kind: ModelStorageKind,
    ) -> Result<()> {
        let conflicted = self
            .kubernetes_storage
            .load_model_storages_by(|k| k.is_unique() && kind == k.to_kind())
            .await?;

        if conflicted.is_empty() {
            Ok(())
        } else {
            bail!(
                "model storage already exists ({name} => {kind}): {list:?}",
                list = conflicted.into_iter().map(|item| item.name_any()).join(","),
            )
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
    ) -> Result<Option<u128>> {
        DatabaseStorageClient::try_new(storage).await.map(|_| None)
    }

    fn validate_model_storage_kubernetes(
        &self,
        storage: &ModelStorageKubernetesSpec,
    ) -> Result<Option<u128>> {
        let ModelStorageKubernetesSpec {} = storage;
        Ok(None)
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn validate_model_storage_object(
        &self,
        name: &str,
        metadata: &ObjectMeta,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Option<u128>> {
        let storage = ModelStorageBindingStorageSpec {
            source: None,
            source_binding_name: None,
            target: storage,
            target_name: name,
        };
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            Some(metadata),
            storage,
            Some(self.prometheus_url),
        )
        .and_then(|client| async move {
            client
                .target()
                .get_capacity_global()
                .await
                .map(|capacity| Some(capacity.capacity.as_u128()))
        })
        .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn bind_model(
        &self,
        binding: &ModelStorageBindingCrd,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        match &storage.target.kind {
            ModelStorageKindSpec::Database(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Database")?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_database(storage, model).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Kubernetes")?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_kubernetes(storage, model)
            }
            ModelStorageKindSpec::ObjectStorage(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::Database(_) => Err("Database"),
                            ModelStorageKindSpec::Kubernetes(_) => Err("Kubernetes"),
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_object(binding, storage, model).await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn bind_model_to_database(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageDatabaseSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        DatabaseStorageClient::try_new(storage.target)
            .await?
            .get_session(model)
            .update_table()
            .await
    }

    fn bind_model_to_kubernetes(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageKubernetesSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        let ModelStorageKubernetesSpec {} = storage.target;
        match model.spec {
            ModelSpec::CustomResourceDefinitionRef(_) => Ok(()),
            _ => bail!("kubernetes storage can only used for CRDs"),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn bind_model_to_object(
        &self,
        binding: &ModelStorageBindingCrd,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        let KubernetesStorageClient { kube, namespace } = self.kubernetes_storage;

        let owner_references = {
            let name = binding.name_any();
            let uid = binding
                .uid()
                .ok_or_else(|| anyhow!("failed to get model storage binding uid: {name}"))?;

            vec![OwnerReference {
                api_version: ModelStorageBindingCrd::api_version(&()).into(),
                block_owner_deletion: Some(true),
                controller: None,
                kind: ModelStorageBindingCrd::kind(&()).into(),
                name,
                uid,
            }]
        };
        let quota = binding.spec.resources.quota();

        ObjectStorageClient::try_new(kube, namespace, None, storage, Some(self.prometheus_url))
            .await?
            .get_session(kube, namespace, model)
            .create_bucket(owner_references, quota)
            .await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub(crate) async fn unbind_model(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageSpec>,
        model: &ModelCrd,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<()> {
        match &storage.target.kind {
            ModelStorageKindSpec::Database(spec) => {
                let storage =
                    ModelStorageBindingStorageSpec {
                        source: assert_source_is_same(storage.source, "Database", |source| {
                            match &source.kind {
                                ModelStorageKindSpec::Database(source) => Ok(source),
                                ModelStorageKindSpec::Kubernetes(_) => Err("Kubernetes"),
                                ModelStorageKindSpec::ObjectStorage(_) => Err("ObjectStorage"),
                            }
                        })?,
                        source_binding_name: storage.source_binding_name,
                        target: spec,
                        target_name: storage.target_name,
                    };
                self.unbind_model_to_database(storage, model, deletion_policy)
                    .await
            }
            ModelStorageKindSpec::Kubernetes(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "Kubernetes", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::Database(_) => Err("Database"),
                            ModelStorageKindSpec::Kubernetes(source) => Ok(source),
                            ModelStorageKindSpec::ObjectStorage(_) => Err("ObjectStorage"),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.unbind_model_to_kubernetes(storage, model, deletion_policy)
            }
            ModelStorageKindSpec::ObjectStorage(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::Database(_) => Err("Database"),
                            ModelStorageKindSpec::Kubernetes(_) => Err("Kubernetes"),
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.unbind_model_to_object(storage, model, deletion_policy)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn unbind_model_to_database(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageDatabaseSpec>,
        model: &ModelCrd,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<()> {
        match deletion_policy {
            ModelStorageBindingDeletionPolicy::Delete => {
                DatabaseStorageClient::try_new(storage.target)
                    .await?
                    .get_session(model)
                    .delete_table()
                    .await
            }
            ModelStorageBindingDeletionPolicy::Retain => Ok(()),
        }
    }

    fn unbind_model_to_kubernetes(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageKubernetesSpec>,
        model: &ModelCrd,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<()> {
        let ModelStorageKubernetesSpec {} = storage.target;
        match deletion_policy {
            ModelStorageBindingDeletionPolicy::Delete => match model.spec {
                ModelSpec::CustomResourceDefinitionRef(_) => Ok(()),
                _ => bail!("kubernetes storage can only used for CRDs"),
            },
            ModelStorageBindingDeletionPolicy::Retain => Ok(()),
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn unbind_model_to_object(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
        deletion_policy: ModelStorageBindingDeletionPolicy,
    ) -> Result<()> {
        let KubernetesStorageClient { kube, namespace } = self.kubernetes_storage;

        let client =
            ObjectStorageClient::try_new(kube, namespace, None, storage, Some(self.prometheus_url))
                .await?;
        let session = client.get_session(kube, namespace, model);
        match deletion_policy {
            ModelStorageBindingDeletionPolicy::Delete => session.delete_bucket().await,
            ModelStorageBindingDeletionPolicy::Retain => {
                session.unsync_bucket(None, false).await.map(|_| ())
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn delete(&self, crd: &ModelStorageCrd) -> Result<()> {
        let bindings = self
            .kubernetes_storage
            .load_model_storage_bindings_by_storage(&crd.name_any())
            .await?;

        if bindings.is_empty() {
            Ok(())
        } else {
            bail!("storage is binded")
        }
    }
}
