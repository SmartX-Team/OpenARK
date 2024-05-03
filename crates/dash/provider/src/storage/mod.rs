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
use dash_api::model_storage_binding::{
    ModelStorageBindingStorageKind, ModelStorageBindingStorageSourceSpec,
    ModelStorageBindingStorageSpec,
};
use dash_api::storage::db::ModelStorageDatabaseSpec;
use dash_api::storage::kubernetes::ModelStorageKubernetesSpec;
use dash_api::storage::object::ModelStorageObjectSpec;
use dash_api::storage::{ModelStorageKindSpec, ModelStorageSpec};
use kube::ResourceExt;
use kube::{core::object::HasStatus, Client};
use serde_json::Value;
use tracing::{instrument, Level};

pub use self::{
    db::DatabaseStorageClient,
    kubernetes::KubernetesStorageClient,
    object::{ObjectStorageClient, ObjectStorageSession},
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
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get(&self, model_name: &str, ref_name: &str) -> Result<Value> {
        let model = self.get_model(model_name).await?;
        for (storage_name, storage) in self.get_model_storage_bindings(model_name).await? {
            let storage = ModelStorageBindingStorageSpec {
                source: storage_name.source().and_then(|(name, _)| {
                    storage.source().map(|(storage, sync_policy)| {
                        ModelStorageBindingStorageSourceSpec {
                            name,
                            storage,
                            sync_policy,
                        }
                    })
                }),
                source_binding_name: storage.source_binding_name(),
                target: storage.target(),
                target_name: storage_name.target(),
            };
            if let Some(value) = self.get_by_storage(storage, &model, ref_name).await? {
                return Ok(value);
            }
        }
        bail!("no such object: {ref_name:?}")
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn list(&self, model_name: &str) -> Result<Vec<Value>> {
        let model = self.get_model(model_name).await?;
        let mut items = vec![];
        for (storage_name, storage) in self.get_model_storage_bindings(model_name).await? {
            let storage = ModelStorageBindingStorageSpec {
                source: storage_name.source().and_then(|(name, _)| {
                    storage.source().map(|(storage, sync_policy)| {
                        ModelStorageBindingStorageSourceSpec {
                            name,
                            storage,
                            sync_policy,
                        }
                    })
                }),
                source_binding_name: storage.source_binding_name(),
                target: storage.target(),
                target_name: storage_name.target(),
            };
            items.append(&mut self.list_by_storage(storage, &model).await?);
        }
        Ok(items)
    }
}

impl<'namespace, 'kube> StorageClient<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip(self, spec), err(Display))]
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

    #[instrument(level = Level::INFO, skip(self, storage, model), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn get_by_storage(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageSpec>,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        match &storage.target.kind {
            ModelStorageKindSpec::Database(target) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Database")?,
                    source_binding_name: storage.source_binding_name,
                    target,
                    target_name: storage.target_name,
                };
                self.get_by_storage_with_database(storage, model, ref_name)
                    .await
            }
            ModelStorageKindSpec::Kubernetes(target) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Kubernetes")?,
                    source_binding_name: storage.source_binding_name,
                    target,
                    target_name: storage.target_name,
                };
                self.get_by_storage_with_kubernetes(storage, model, ref_name)
                    .await
            }
            ModelStorageKindSpec::ObjectStorage(target) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::Database(_) => Err("Database"),
                            ModelStorageKindSpec::Kubernetes(_) => Err("Kubernetes"),
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target,
                    target_name: storage.target_name,
                };
                self.get_by_storage_with_object(storage, model, ref_name)
                    .await
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self, storage, model), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn get_by_storage_with_database(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageDatabaseSpec>,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        DatabaseStorageClient::try_new(storage.target)
            .await?
            .get_session(model)
            .get(ref_name)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, storage, model), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn get_by_storage_with_kubernetes(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageKubernetesSpec>,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        let ModelStorageKubernetesSpec {} = storage.target;
        match &model.spec {
            ModelSpec::Dynamic {} => Ok(None),
            ModelSpec::Fields(_) => Ok(None),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.get_custom_resource(model, spec, ref_name).await
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self, storage, model), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn get_by_storage_with_object(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
        ref_name: &str,
    ) -> Result<Option<Value>> {
        ObjectStorageClient::try_new(self.kube, self.namespace, storage)
            .await?
            .get_session(self.kube, self.namespace, model)
            .get(ref_name)
            .await
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
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

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get_model(&self, model_name: &str) -> Result<ModelCrd> {
        let storage = KubernetesStorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };
        storage.load_model(model_name).await
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get_model_storage_bindings(
        &self,
        model_name: &str,
    ) -> Result<
        Vec<(
            ModelStorageBindingStorageKind<String>,
            ModelStorageBindingStorageKind<ModelStorageSpec>,
        )>,
    > {
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
    #[instrument(level = Level::INFO, skip(self, storage), err(Display))]
    async fn list_by_storage(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageSpec>,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        match &storage.target.kind {
            ModelStorageKindSpec::Database(target) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Database")?,
                    source_binding_name: storage.source_binding_name,
                    target,
                    target_name: storage.target_name,
                };
                self.list_by_storage_with_database(storage, model).await
            }
            ModelStorageKindSpec::Kubernetes(target) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_none(storage.source, "Kubernetes")?,
                    source_binding_name: storage.source_binding_name,
                    target,
                    target_name: storage.target_name,
                };
                self.list_by_storage_with_kubernetes(storage, model).await
            }
            ModelStorageKindSpec::ObjectStorage(target) => {
                let storage = ModelStorageBindingStorageSpec {
                    source: assert_source_is_same(storage.source, "ObjectStorage", |source| {
                        match &source.kind {
                            ModelStorageKindSpec::Database(_) => Err("Database"),
                            ModelStorageKindSpec::Kubernetes(_) => Err("Kubernetes"),
                            ModelStorageKindSpec::ObjectStorage(source) => Ok(source),
                        }
                    })?,
                    source_binding_name: storage.source_binding_name,
                    target,
                    target_name: storage.target_name,
                };
                self.list_by_storage_with_object(storage, model).await
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self, storage), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn list_by_storage_with_database(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageDatabaseSpec>,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        DatabaseStorageClient::try_new(storage.target)
            .await?
            .get_session(model)
            .get_list()
            .await
    }

    #[instrument(level = Level::INFO, skip(self, storage), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn list_by_storage_with_kubernetes(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageKubernetesSpec>,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        let ModelStorageKubernetesSpec {} = storage.target;
        match &model.spec {
            ModelSpec::Dynamic {} => Ok(Default::default()),
            ModelSpec::Fields(_) => Ok(Default::default()),
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.list_custom_resource(model, spec).await
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self, storage), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
    async fn list_by_storage_with_object(
        &self,
        storage: ModelStorageBindingStorageSpec<'_, &ModelStorageObjectSpec>,
        model: &ModelCrd,
    ) -> Result<Vec<Value>> {
        ObjectStorageClient::try_new(self.kube, self.namespace, storage)
            .await?
            .get_session(self.kube, self.namespace, model)
            .get_list()
            .await
    }

    #[instrument(level = Level::INFO, skip(self), fields(model.name = %model.name_any(), model.namespace = model.namespace()), err(Display))]
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

pub fn assert_source_is_none<T, R>(source: Option<T>, name: &'static str) -> Result<Option<R>> {
    if source.is_some() {
        bail!("Sync to {name} is not supported")
    } else {
        Ok(None)
    }
}

pub fn assert_source_is_same<'a, T, R>(
    source: Option<ModelStorageBindingStorageSourceSpec<'a, T>>,
    name: &'static str,
    map: impl FnOnce(T) -> Result<R, &'static str>,
) -> Result<Option<ModelStorageBindingStorageSourceSpec<'a, R>>> {
    source
        .map(
            |ModelStorageBindingStorageSourceSpec {
                 name: source_name,
                 storage: source,
                 sync_policy,
             }| match map(source) {
                Ok(source) => Ok(ModelStorageBindingStorageSourceSpec {
                    name: source_name,
                    storage: source,
                    sync_policy,
                }),
                Err(source) => {
                    bail!("Sync to {name} from other source ({source}) is not supported")
                }
            },
        )
        .transpose()
}

fn get_model_fields_parsed(model: &ModelCrd) -> &ModelFieldsNativeSpec {
    model.status().unwrap().fields.as_ref().unwrap()
}
