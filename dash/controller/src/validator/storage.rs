use anyhow::{bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    model_storage_binding::ModelStorageBindingStorageSpec,
    storage::{
        db::ModelStorageDatabaseSpec, kubernetes::ModelStorageKubernetesSpec,
        object::ModelStorageObjectSpec, ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec,
        ModelStorageSpec,
    },
};
use dash_provider::storage::{
    assert_source_is_none, assert_source_is_same, DatabaseStorageClient, KubernetesStorageClient,
    ObjectStorageClient,
};
use itertools::Itertools;
use kube::ResourceExt;

pub struct ModelStorageValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> ModelStorageValidator<'namespace, 'kube> {
    pub async fn validate_model_storage(&self, name: &str, spec: &ModelStorageSpec) -> Result<()> {
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
                self.validate_model_storage_object(name, spec).await
            }
        }
    }

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

    async fn validate_model_storage_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
    ) -> Result<()> {
        DatabaseStorageClient::try_new(storage).await.map(|_| ())
    }

    fn validate_model_storage_kubernetes(
        &self,
        storage: &ModelStorageKubernetesSpec,
    ) -> Result<()> {
        let ModelStorageKubernetesSpec {} = storage;
        Ok(())
    }

    async fn validate_model_storage_object(
        &self,
        name: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<()> {
        let storage = ModelStorageBindingStorageSpec {
            source: None,
            target: storage,
            target_name: name,
        };
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            storage,
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn bind_model(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageCrd>,
        model: &ModelCrd,
    ) -> Result<()> {
        match &storage.target.spec.kind {
            ModelStorageKindSpec::Database(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Database")?,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_database(storage, model).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Kubernetes")?,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_kubernetes(storage, model)
            }
            ModelStorageKindSpec::ObjectStorage(spec) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.spec.kind {
                            ModelStorageKindSpec::Database(_) => Err("Database"),
                            ModelStorageKindSpec::Kubernetes(_) => Err("Kubernetes"),
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    target: spec,
                    target_name: storage.target_name,
                };
                self.bind_model_to_object(storage, model).await
            }
        }
    }

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

    async fn bind_model_to_object(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
    ) -> Result<()> {
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            storage,
        )
        .await?
        .get_session(model)
        .create_bucket()
        .await
    }
}
