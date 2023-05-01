mod db;
mod kubernetes;

use anyhow::{bail, Result};
use dash_api::model::ModelFieldsNativeSpec;
use dash_api::model::{
    ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldKindExtendedSpec, ModelFieldKindSpec,
    ModelFieldSpec, ModelSpec,
};
use dash_api::storage::ModelStorageSpec;
use kube::{core::object::HasStatus, Client};
use serde_json::Value;

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
                    self.get_by_model(model_name, ref_name).await
                }
            },
        }
    }

    pub async fn get_by_model(&self, model_name: &str, ref_name: &str) -> Result<Value> {
        let model = self.get_model(model_name).await?;
        for storage in self.get_model_storage_bindings(model_name).await? {
            if let Some(value) = self.get_by_storage_with(&storage, &model, ref_name).await? {
                return Ok(value);
            }
        }
        bail!("no such object: {ref_name:?}")
    }

    async fn get_by_storage_with(
        &self,
        _storage: &ModelStorageSpec,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        match &model.spec {
            // TODO: to be implemented (i.g. Access to Database)
            ModelSpec::Fields(_spec) => todo!(),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.get_custom_resource(model, spec, ref_name).await
            }
        }
    }

    async fn get_custom_resource(
        &self,
        model: &ModelCrd,
        spec: &ModelCustomResourceDefinitionRefSpec,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        let parsed = get_model_fields_parsed(model);

        let storage = KubernetesStorageClient { kube: self.kube };
        storage
            .load_custom_resource(spec, parsed, self.namespace, ref_name)
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

impl<'namespace, 'kube> StorageClient<'namespace, 'kube> {
    pub async fn list(&self, spec: Option<&ModelFieldSpec>) -> Result<Vec<Value>> {
        match spec.map(|spec| &spec.kind) {
            None | Some(ModelFieldKindSpec::Native(_)) => {
                bail!("getting native value from storage is not supported")
            }
            Some(ModelFieldKindSpec::Extended(kind)) => match kind {
                // BEGIN reference types
                ModelFieldKindExtendedSpec::Model { name: model_name } => {
                    self.list_by_model(model_name).await
                }
            },
        }
    }

    pub async fn list_by_model(&self, model_name: &str) -> Result<Vec<Value>> {
        let model = self.get_model(model_name).await?;
        let mut items = vec![];
        for storage in self.get_model_storage_bindings(model_name).await? {
            items.append(&mut self.list_by_storage_with(&storage, &model).await?);
        }
        Ok(items)
    }

    async fn list_by_storage_with(
        &self,
        _storage: &ModelStorageSpec,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        match &model.spec {
            // TODO: to be implemented (i.g. Access to Database)
            ModelSpec::Fields(_spec) => todo!(),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.list_custom_resource(model, spec).await
            }
        }
    }

    async fn list_custom_resource(
        &self,
        model: &ModelCrd,
        spec: &ModelCustomResourceDefinitionRefSpec,
    ) -> Result<Vec<Value>> {
        let parsed = get_model_fields_parsed(model);

        let storage = KubernetesStorageClient { kube: self.kube };
        storage
            .load_custom_resource_all(spec, parsed, self.namespace)
            .await
    }
}

fn get_model_fields_parsed(model: &ModelCrd) -> &ModelFieldsNativeSpec {
    model.status().unwrap().fields.as_ref().unwrap()
}
