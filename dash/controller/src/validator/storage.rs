use anyhow::{bail, Result};
use dash_api::{
    model::{ModelCrd, ModelSpec},
    storage::{
        db::ModelStorageDatabaseSpec, kubernetes::ModelStorageKubernetesSpec,
        object::ModelStorageObjectSpec, ModelStorageCrd, ModelStorageKindSpec, ModelStorageSpec,
    },
};
use dash_provider::storage::{DatabaseStorageClient, KubernetesStorageClient, ObjectStorageClient};

pub struct ModelStorageValidator<'namespace, 'kube> {
    pub kubernetes_storage: KubernetesStorageClient<'namespace, 'kube>,
}

impl<'namespace, 'kube> ModelStorageValidator<'namespace, 'kube> {
    pub async fn validate_model_storage(&self, spec: &ModelStorageSpec) -> Result<()> {
        match &spec.kind {
            ModelStorageKindSpec::Database(spec) => {
                self.validate_model_storage_database(spec).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => self.validate_model_storage_kubernetes(spec),
            ModelStorageKindSpec::ObjectStorage(spec) => {
                self.validate_model_storage_object(spec).await
            }
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

    async fn validate_model_storage_object(&self, storage: &ModelStorageObjectSpec) -> Result<()> {
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
        storage: &ModelStorageCrd,
        model: &ModelCrd,
    ) -> Result<()> {
        match &storage.spec.kind {
            ModelStorageKindSpec::Database(storage) => {
                self.bind_model_to_database(storage, model).await
            }
            ModelStorageKindSpec::Kubernetes(storage) => {
                self.bind_model_to_kubernetes(storage, model)
            }
            ModelStorageKindSpec::ObjectStorage(storage) => {
                self.bind_model_to_object(storage, model).await
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
