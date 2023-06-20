mod db;
mod kubernetes;
mod object;

use anyhow::{bail, Result};
use async_trait::async_trait;
use dash_api::model::ModelFieldsNativeSpec;
use dash_api::model::{
    ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldKindExtendedSpec, ModelFieldKindSpec,
    ModelFieldSpec, ModelSpec,
};
use dash_api::storage::db::ModelStorageDatabaseSpec;
use dash_api::storage::kubernetes::ModelStorageKubernetesSpec;
use dash_api::storage::object::ModelStorageObjectSpec;
use dash_api::storage::{ModelStorageKindSpec, ModelStorageSpec};
use kube::{core::object::HasStatus, Client};
use serde_json::Value;

pub use self::{
    db::DatabaseStorageClient, kubernetes::KubernetesStorageClient, object::ObjectStorageClient,
};

#[async_trait]
pub trait Storage {
    async fn get(&self, model_name: &str, ref_name: &str) -> Result<Value>;

    async fn list(&self, model_name: &str) -> Result<Vec<Value>>;
}

pub struct StorageClient<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

#[async_trait]
impl<'namespace, 'kube> Storage for StorageClient<'namespace, 'kube> {
    async fn get(&self, model_name: &str, ref_name: &str) -> Result<Value> {
        let model = self.get_model(model_name).await?;
        for (storage_name, storage) in self.get_model_storage_bindings(model_name).await? {
            if let Some(value) = self
                .get_by_storage(&storage, &storage_name, &model, ref_name)
                .await?
            {
                return Ok(value);
            }
        }
        bail!("no such object: {ref_name:?}")
    }

    async fn list(&self, model_name: &str) -> Result<Vec<Value>> {
        let model = self.get_model(model_name).await?;
        let mut items = vec![];
        for (storage_name, storage) in self.get_model_storage_bindings(model_name).await? {
            items.append(
                &mut self
                    .list_by_storage(&storage, &storage_name, &model)
                    .await?,
            );
        }
        Ok(items)
    }
}

impl<'namespace, 'kube> StorageClient<'namespace, 'kube> {
    pub(crate) async fn get_by_field(
        &self,
        spec: Option<&ModelFieldSpec>,
        ref_name: &str,
    ) -> Result<Value> {
        match spec.map(|spec| &spec.kind) {
            None | Some(ModelFieldKindSpec::Native(_)) => {
                bail!("getting native value from storage is not supported: {ref_name:?}")
            }
            Some(ModelFieldKindSpec::Extended(kind)) => match kind {
                // BEGIN reference types
                ModelFieldKindExtendedSpec::Model { name: model_name } => {
                    self.get(model_name.as_str(), ref_name).await
                }
            },
        }
    }

    async fn get_by_storage(
        &self,
        storage: &ModelStorageSpec,
        storage_name: &str,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        match &storage.kind {
            ModelStorageKindSpec::Database(storage) => {
                self.get_by_storage_with_database(storage, model, ref_name)
                    .await
            }
            ModelStorageKindSpec::Kubernetes(storage) => {
                self.get_by_storage_with_kubernetes(storage, model, ref_name)
                    .await
            }
            ModelStorageKindSpec::ObjectStorage(storage) => {
                self.get_by_storage_with_object(storage, storage_name, model, ref_name)
                    .await
            }
        }
    }

    async fn get_by_storage_with_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        DatabaseStorageClient::try_new(storage)
            .await?
            .get_session(model)
            .get(ref_name)
            .await
    }

    async fn get_by_storage_with_kubernetes(
        &self,
        ModelStorageKubernetesSpec {}: &ModelStorageKubernetesSpec,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        match &model.spec {
            ModelSpec::Fields(_) => Ok(None),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.get_custom_resource(model, spec, ref_name).await
            }
        }
    }

    async fn get_by_storage_with_object(
        &self,
        storage: &ModelStorageObjectSpec,
        storage_name: &str,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        ObjectStorageClient::try_new(self.kube, self.namespace, storage_name, storage)
            .await?
            .get_session(model)
            .get(ref_name)
            .await
    }

    async fn get_custom_resource(
        &self,
        model: &ModelCrd,
        spec: &ModelCustomResourceDefinitionRefSpec,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        let parsed = get_model_fields_parsed(model);

        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };
        storage.load_custom_resource(spec, parsed, ref_name).await
    }

    async fn get_model(&self, model_name: &str) -> Result<ModelCrd> {
        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };
        storage.load_model(model_name).await
    }

    async fn get_model_storage_bindings(
        &self,
        model_name: &str,
    ) -> Result<Vec<(String, ModelStorageSpec)>> {
        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };

        let storages = storage.load_model_storage_bindings(model_name).await?;
        if storages.is_empty() {
            bail!("model has not been binded: {model_name:?}")
        }
        Ok(storages)
    }
}

impl<'namespace, 'kube> StorageClient<'namespace, 'kube> {
    async fn list_by_storage(
        &self,
        storage: &ModelStorageSpec,
        storage_name: &str,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        match &storage.kind {
            ModelStorageKindSpec::Database(storage) => {
                self.list_by_storage_with_database(storage, model).await
            }
            ModelStorageKindSpec::Kubernetes(storage) => {
                self.list_by_storage_with_kubernetes(storage, model).await
            }
            ModelStorageKindSpec::ObjectStorage(storage) => {
                self.list_by_storage_with_object(storage, storage_name, model)
                    .await
            }
        }
    }

    async fn list_by_storage_with_database(
        &self,
        storage: &ModelStorageDatabaseSpec,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        DatabaseStorageClient::try_new(storage)
            .await?
            .get_session(model)
            .get_list()
            .await
    }

    async fn list_by_storage_with_kubernetes(
        &self,
        ModelStorageKubernetesSpec {}: &ModelStorageKubernetesSpec,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        match &model.spec {
            ModelSpec::Fields(_) => Ok(Default::default()),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.list_custom_resource(model, spec).await
            }
        }
    }

    async fn list_by_storage_with_object(
        &self,
        storage: &ModelStorageObjectSpec,
        storage_name: &str,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        ObjectStorageClient::try_new(self.kube, self.namespace, storage_name, storage)
            .await?
            .get_session(model)
            .get_list()
            .await
    }

    async fn list_custom_resource(
        &self,
        model: &ModelCrd,
        spec: &ModelCustomResourceDefinitionRefSpec,
    ) -> Result<Vec<Value>> {
        let parsed = get_model_fields_parsed(model);

        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };
        storage.load_custom_resource_all(spec, parsed).await
    }
}

fn get_model_fields_parsed(model: &ModelCrd) -> &ModelFieldsNativeSpec {
    model.status().unwrap().fields.as_ref().unwrap()
}
