use dash_api::{
    model::{ModelCrd, ModelSpec},
    storage::{
        db::ModelStorageDatabaseSpec, kubernetes::ModelStorageKubernetesSpec, ModelStorageCrd,
        ModelStorageKindSpec, ModelStorageSpec,
    },
};
use dash_provider::storage::{DatabaseStorageClient, KubernetesStorageClient};
use ipis::core::anyhow::{bail, Result};

pub struct ModelStorageValidator<'a> {
    pub kubernetes_storage: KubernetesStorageClient<'a>,
}

impl<'a> ModelStorageValidator<'a> {
    pub async fn validate_model_storage(&self, spec: &ModelStorageSpec) -> Result<()> {
        match &spec.kind {
            ModelStorageKindSpec::Database(spec) => {
                self.validate_model_storage_database(spec).await
            }
            ModelStorageKindSpec::Kubernetes(spec) => self.validate_model_storage_kubernetes(spec),
        }
    }

    async fn validate_model_storage_database(&self, spec: &ModelStorageDatabaseSpec) -> Result<()> {
        DatabaseStorageClient::load_storage(spec).await.map(|_| ())
    }

    fn validate_model_storage_kubernetes(&self, spec: &ModelStorageKubernetesSpec) -> Result<()> {
        let ModelStorageKubernetesSpec {} = spec;
        Ok(())
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
        }
    }

    async fn bind_model_to_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
        model: &ModelCrd,
    ) -> Result<()> {
        let database_storage = DatabaseStorageClient::try_new(storage, model).await?;
        database_storage.create_table().await
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
}
