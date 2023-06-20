use anyhow::{bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    storage::{
        db::ModelStorageDatabaseSpec, kubernetes::ModelStorageKubernetesSpec,
        object::ModelStorageObjectSpec, ModelStorageCrd, ModelStorageKind, ModelStorageKindSpec,
        ModelStorageSpec,
    },
};
use dash_provider::storage::{DatabaseStorageClient, KubernetesStorageClient, ObjectStorageClient};
use itertools::Itertools;
use kube::ResourceExt;

pub struct ModelStorageValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> ModelStorageValidator<'namespace, 'kube> {
    pub async fn validate_model_storage(&self, name: &str, spec: &ModelStorageSpec) -> Result<()> {
        self.validate_model_storage_conflict(name, spec.kind.to_kind())
            .await?;

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
            .load_model_storages_by(|k| kind == k.to_kind())
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
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            name,
            storage,
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn bind_model(
        &self,
        storage: &ModelStorageCrd,
        model: &ModelCrd,
    ) -> Result<()> {
        match &storage.spec.kind {
            ModelStorageKindSpec::Database(spec) => self.bind_model_to_database(spec, model).await,
            ModelStorageKindSpec::Kubernetes(spec) => self.bind_model_to_kubernetes(spec, model),
            ModelStorageKindSpec::ObjectStorage(spec) => {
                self.bind_model_to_object(spec, &storage.name_any(), model)
                    .await
            }
        }
    }

    async fn bind_model_to_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
        model: &ModelCrd,
    ) -> Result<()> {
        DatabaseStorageClient::try_new(storage)
            .await?
            .get_session(model)
            .update_table()
            .await
    }

    fn bind_model_to_kubernetes(
        &self,
        storage: &ModelStorageKubernetesSpec,
        model: &ModelCrd,
    ) -> Result<()> {
        let ModelStorageKubernetesSpec {} = storage;
        match model.spec {
            ModelSpec::CustomResourceDefinitionRef(_) => Ok(()),
            _ => bail!("kubernetes storage can only used for CRDs"),
        }
    }

    async fn bind_model_to_object(
        &self,
        storage: &ModelStorageObjectSpec,
        storage_name: &str,
        model: &ModelCrd,
    ) -> Result<()> {
        ObjectStorageClient::try_new(
            self.kubernetes_storage.kube,
            self.kubernetes_storage.namespace,
            storage_name,
            storage,
        )
        .await?
        .get_session(model)
        .create_bucket()
        .await
    }
}
