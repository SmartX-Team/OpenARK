mod db;
mod kubernetes;

use dash_api::model::{
    ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldKindExtendedSpec, ModelFieldKindSpec,
    ModelFieldSpec, ModelSpec,
};
use dash_api::storage::ModelStorageSpec;
use ipis::core::anyhow::{bail, Result};
use kiss_api::{kube::Client, serde_json::Value};

pub use self::db::DatabaseStorageClient;
pub use self::kubernetes::KubernetesStorageClient;

pub struct StorageClient<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> StorageClient<'namespace, 'kube> {
    pub async fn get(&self, spec: Option<&ModelFieldSpec>, ref_name: &str) -> Result<Value> {
        match spec.map(|spec| &spec.kind) {
            None | Some(ModelFieldKindSpec::Native(_)) => {
                bail!("getting native value from storage is not supported: {ref_name:?}")
            }
            Some(ModelFieldKindSpec::Extended(kind)) => match kind {
                // BEGIN reference types
                ModelFieldKindExtendedSpec::Model { name: model_name } => {
                    let model = self.get_model(model_name).await?;
                    for storage in self.get_model_storage_bindings(model_name).await? {
                        if let Some(value) =
                            self.get_by_storage(&storage, &model.spec, ref_name).await?
                        {
                            return Ok(value);
                        }
                    }
                    bail!("no such object: {ref_name:?}")
                }
            },
        }
    }

    async fn get_by_storage(
        &self,
        storage: &ModelStorageSpec,
        model: &ModelSpec,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        match model {
            // TODO: to be implemented (i.g. Access to Database)
            ModelSpec::Fields(spec) => todo!(),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.get_custom_resource(spec, ref_name).await
            }
        }
    }

    async fn get_custom_resource(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        let storage = KubernetesStorageClient { kube: self.kube };
        storage
            .load_custom_resource(spec, self.namespace, ref_name)
            .await
    }

    async fn get_model(&self, model_name: &str) -> Result<ModelCrd> {
        let storage = KubernetesStorageClient { kube: self.kube };
        storage.load_model(model_name).await
    }

    async fn get_model_storage_bindings(&self, model_name: &str) -> Result<Vec<ModelStorageSpec>> {
        let storage = KubernetesStorageClient { kube: self.kube };

        let storages = storage.load_model_storage_bindings(model_name).await?;
        if storages.is_empty() {
            bail!("model has not been binded: {model_name:?}")
        }
        Ok(storages)
    }
}
