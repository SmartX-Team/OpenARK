use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use dash_api::{
    model::{
        ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldsNativeSpec, ModelSpec,
        ModelState,
    },
    model_claim::{ModelClaimCrd, ModelClaimState},
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy, ModelStorageBindingSpec,
        ModelStorageBindingState, ModelStorageBindingStatus, ModelStorageBindingStorageKind,
    },
    storage::{ModelStorageCrd, ModelStorageKindSpec, ModelStorageState},
    task::{TaskActorSourceConfigMapRefSpec, TaskCrd, TaskState},
};
use futures::{stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;
use k8s_openapi::{
    api::core::v1::ConfigMap,
    apiextensions_apiserver::pkg::apis::apiextensions::v1::{
        CustomResourceDefinition, CustomResourceDefinitionVersion,
    },
    ClusterResourceScope, NamespaceResourceScope,
};
use kube::{
    api::{DeleteParams, ListParams, PostParams},
    core::{object::HasStatus, DynamicObject, ObjectMeta},
    discovery, Api, Client, Resource, ResourceExt,
};
use maplit::btreemap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;
use tracing::{instrument, Level};

use crate::input::{InputFieldValue, ItemTemplate};

#[derive(Copy, Clone)]
pub struct KubernetesStorageClient<'namespace, 'kube> {
    pub namespace: &'namespace str,
    pub kube: &'kube Client,
}

impl<'namespace, 'kube> KubernetesStorageClient<'namespace, 'kube> {
    const LABEL_SUBJECT: &'static str = "dash.ulagbulag.io/subject";

    fn api_all<K>(&self) -> Api<K>
    where
        K: Resource<Scope = ClusterResourceScope>,
        <K as Resource>::DynamicType: Default,
    {
        Api::all(self.kube.clone())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub(super) async fn api_custom_resource(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        resource_name: Option<&str>,
    ) -> Result<Api<DynamicObject>> {
        let (api_group, scope, def) = self.load_custom_resource_definition(spec).await?;
        let plural = spec.plural();

        // Discover most stable version variant of document
        let apigroup = discovery::group(self.kube, &api_group).await?;

        let ar = match apigroup
            .versioned_resources(&def.name)
            .into_iter()
            .find(|(ar, _)| ar.plural == plural)
        {
            Some((ar, _)) => ar,
            None => {
                let model_name = &spec.name;
                bail!("no such CRD: {model_name:?}")
            }
        };

        // Use the discovered kind in an Api, and Controller with the ApiResource as its DynamicType
        match scope.as_str() {
            "Namespaced" => Ok(Api::namespaced_with(self.kube.clone(), self.namespace, &ar)),
            "Cluster" => Ok(Api::all_with(self.kube.clone(), &ar)),
            scope => match resource_name {
                Some(resource_name) => bail!("cannot infer CRD scope {scope:?}: {resource_name:?}"),
                None => bail!("cannot infer CRD scope {scope:?}"),
            },
        }
    }

    fn api_namespaced<K>(&self) -> Api<K>
    where
        K: Resource<Scope = NamespaceResourceScope>,
        <K as Resource>::DynamicType: Default,
    {
        let client = self.kube.clone();
        match self.namespace {
            "*" => Api::all(client),
            namespace => Api::namespaced(client, namespace),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_config_map<'f>(
        &self,
        spec: &'f TaskActorSourceConfigMapRefSpec,
    ) -> Result<(&'f str, String)> {
        let TaskActorSourceConfigMapRefSpec { name, path } = spec;

        let api = self.api_namespaced::<ConfigMap>();
        let config_map = api.get(name).await?;

        match config_map.data.and_then(|mut data| data.remove(path)) {
            Some(content) => Ok((path, content)),
            None => bail!(
                "no such file in ConfigMap: {path:?} in {namespace}::{name}",
                namespace = self.namespace,
            ),
        }
    }

    #[instrument(level = Level::INFO, skip(self, parsed), err(Display))]
    pub async fn load_custom_resource(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        parsed: &ModelFieldsNativeSpec,
        resource_name: &str,
    ) -> Result<Option<Value>> {
        let api = self.api_custom_resource(spec, Some(resource_name)).await?;
        api.get_opt(resource_name)
            .await?
            .map(|item| convert_model_item(item, parsed))
            .transpose()
    }

    #[instrument(level = Level::INFO, skip(self, parsed), err(Display))]
    pub async fn load_custom_resource_all(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        parsed: &ModelFieldsNativeSpec,
    ) -> Result<Vec<Value>> {
        let api = self.api_custom_resource(spec, None).await?;
        let lp = ListParams::default();
        api.list(&lp).await.map_err(Into::into).and_then(|list| {
            list.items
                .into_iter()
                .map(|item| convert_model_item(item, parsed))
                .collect()
        })
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_custom_resource_definition(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
    ) -> Result<(String, String, CustomResourceDefinitionVersion)> {
        let (api_group, version) = crate::imp::parse_api_version(&spec.name)?;

        let api = self.api_all::<CustomResourceDefinition>();
        let crd = api.get(api_group).await?;

        match crd.spec.versions.iter().find(|def| def.name == version) {
            Some(def) => Ok((crd.spec.group, crd.spec.scope, def.clone())),
            None => bail!(
                "CRD version is invalid; expected one of {:?}, but given {version}",
                crd.spec.versions.iter().map(|def| &def.name).join(","),
            ),
        }
    }
}

impl<'namespace, 'kube> KubernetesStorageClient<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn delete_model(&self, name: &str) -> Result<()> {
        let api = self.api_namespaced::<ModelCrd>();
        match self.try_load_model(&api, name).await? {
            Some(_) => {
                self.delete_model_storage_binding_by_model(name).await?;
                sleep(Duration::from_secs(3)).await;

                let dp = DeleteParams::foreground();
                api.delete(name, &dp).await.map(|_| ()).map_err(Into::into)
            }
            None => Ok(()),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn try_load_model(&self, api: &Api<ModelCrd>, name: &str) -> Result<Option<ModelCrd>> {
        let model = match api.get_opt(name).await? {
            Some(model) => model,
            None => return Ok(None),
        };

        match &model.status {
            Some(status) if status.state == ModelState::Ready => match &status.fields {
                Some(_) => Ok(Some(model)),
                None => bail!("model has no parsed fields: {name:?}"),
            },
            Some(_) | None => bail!("model is not ready: {name:?}"),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model(&self, name: &str) -> Result<ModelCrd> {
        let api = self.api_namespaced::<ModelCrd>();
        self.try_load_model(&api, name)
            .await?
            .ok_or_else(|| anyhow!("no such model: {name:?}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model_all(&self) -> Result<Vec<ResourceRef>> {
        let api = self.api_namespaced::<ModelCrd>();
        let lp = ListParams::default();
        let models = api.list(&lp).await?;

        Ok(models
            .into_iter()
            .filter(|model| {
                model
                    .status()
                    .map(|status| {
                        matches!(status.state, ModelState::Ready) && status.fields.is_some()
                    })
                    .unwrap_or_default()
            })
            .map(|model| ResourceRef {
                name: model.name_any(),
                namespace: model.namespace().unwrap(),
            })
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model_or_create_as_dynamic(
        &self,
        field_manager: &str,
        name: &str,
    ) -> Result<ModelCrd> {
        let api = self.api_namespaced::<ModelCrd>();
        match self.try_load_model(&api, name).await? {
            Some(model) => return Ok(model),
            None => {
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some(field_manager.into()),
                };
                let data = ModelCrd {
                    metadata: ObjectMeta {
                        name: Some(name.into()),
                        namespace: Some(self.namespace.into()),
                        labels: Some(btreemap! {
                            "dash.ulagbulag.io/claimed-by".into() => name.into(),
                            "dash.ulagbulag.io/managed-by".into() => field_manager.into(),
                        }),
                        ..Default::default()
                    },
                    spec: ModelSpec::Dynamic {},
                    status: None,
                };

                // skipping validating: dynamic model should be valid
                api.create(&pp, &data).await.map_err(Into::into)
            }
        }
    }
}

impl<'namespace, 'kube> KubernetesStorageClient<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model_claim(&self, name: &str) -> Result<Option<ModelClaimCrd>> {
        let api = self.api_namespaced::<ModelClaimCrd>();
        let claim = match api.get_opt(name).await? {
            Some(claim) => claim,
            None => return Ok(None),
        };

        match &claim.status {
            Some(status)
                if matches!(
                    status.state,
                    ModelClaimState::Ready | ModelClaimState::Replacing | ModelClaimState::Deleting,
                ) =>
            {
                Ok(Some(claim))
            }
            Some(_) | None => bail!("model claim is not ready: {name:?}"),
        }
    }
}

impl<'namespace, 'kube> KubernetesStorageClient<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model_storage(&self, name: &str) -> Result<ModelStorageCrd> {
        let api = self.api_namespaced::<ModelStorageCrd>();
        let storage = api.get(name).await?;

        match &storage.status {
            Some(status) if status.state == ModelStorageState::Ready => Ok(storage),
            Some(_) | None => bail!("model storage is not ready: {name:?}"),
        }
    }

    #[instrument(level = Level::INFO, skip(self, filter), err(Display))]
    pub async fn load_model_storages_by<Filter>(
        &self,
        filter: Filter,
    ) -> Result<Vec<ModelStorageCrd>>
    where
        Filter: Fn(&ModelStorageKindSpec) -> bool,
    {
        let api = self.api_namespaced::<ModelStorageCrd>();
        let lp = ListParams::default();

        api.list(&lp)
            .await
            .map(|list| {
                list.items
                    .into_iter()
                    .filter(|item| {
                        item.status
                            .as_ref()
                            .and_then(|status| status.kind.as_ref())
                            .map(&filter)
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .map_err(Into::into)
    }
}

impl<'namespace, 'kube> KubernetesStorageClient<'namespace, 'kube> {
    pub async fn create_model_storage_binding(
        &self,
        field_manager: &str,
        model_name: String,
        storage: ModelStorageBindingStorageKind<String>,
    ) -> Result<ModelStorageBindingCrd> {
        let api = self.api_namespaced::<ModelStorageBindingCrd>();
        let pp = PostParams {
            dry_run: false,
            field_manager: Some(field_manager.into()),
        };
        let data = ModelStorageBindingCrd {
            metadata: ObjectMeta {
                name: Some(model_name.clone()),
                namespace: Some(self.namespace.into()),
                labels: Some(btreemap! {
                    "dash.ulagbulag.io/managed-by".into() => field_manager.into(),
                }),
                ..Default::default()
            },
            spec: ModelStorageBindingSpec {
                // NOTE: deleting original data is only permitted for humans
                deletion_policy: ModelStorageBindingDeletionPolicy::Retain,
                model: model_name,
                storage,
            },
            status: None,
        };

        api.create(&pp, &data).await.map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn delete_model_storage_binding_by_model(&self, model_name: &str) -> Result<()> {
        let api = self.api_namespaced::<ModelStorageBindingCrd>();
        let dp = DeleteParams::background();

        self.load_model_storage_bindings_all(&api)
            .await?
            .into_iter()
            .filter(|binding| binding.spec.model == model_name)
            .map(|binding| {
                let api = api.clone();
                let dp = dp.clone();
                async move { api.delete(&binding.name_any(), &dp).await }
            })
            .collect::<FuturesUnordered<_>>()
            .try_collect::<Vec<_>>()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model_storage_bindings(
        &self,
        model_name: &str,
    ) -> Result<Vec<(ObjectMeta, ModelStorageBindingStatus)>> {
        let api = self.api_namespaced::<ModelStorageBindingCrd>();

        Ok(self
            .load_model_storage_bindings_all(&api)
            .await?
            .into_iter()
            .filter(|binding| binding.spec.model == model_name)
            .filter_map(|binding| binding.status.map(|status| (binding.metadata, status)))
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn load_model_storage_bindings_all(
        &self,
        api: &Api<ModelStorageBindingCrd>,
    ) -> Result<Vec<ModelStorageBindingCrd>> {
        let lp = ListParams::default();
        let bindings = api.list(&lp).await?;

        Ok(bindings
            .items
            .into_iter()
            .filter(|binding| {
                binding
                    .status()
                    .map(|status| matches!(status.state, ModelStorageBindingState::Ready))
                    .unwrap_or_default()
            })
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_model_storage_bindings_by_storage(
        &self,
        storage_name: &str,
    ) -> Result<Vec<ModelStorageBindingStatus>> {
        let api = self.api_namespaced::<ModelStorageBindingCrd>();

        Ok(self
            .load_model_storage_bindings_all(&api)
            .await?
            .into_iter()
            .filter(|binding| {
                let storage = &binding.spec.storage;
                storage.source().map(|(name, _)| name.as_str()) == Some(storage_name)
                    || storage.target() == storage_name
            })
            .filter_map(|binding| binding.status)
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn ensure_model_storage_binding(&self, model_name: &str) -> Result<()> {
        let client = super::StorageClient {
            namespace: self.namespace,
            kube: self.kube,
        };

        client
            .get_model_storage_bindings(model_name)
            .await
            .and_then(|bindings| {
                if bindings.is_empty() {
                    bail!("model is not binded yet: {model_name}")
                } else {
                    Ok(())
                }
            })
    }
}

impl<'namespace, 'kube> KubernetesStorageClient<'namespace, 'kube> {
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_task(&self, name: &str) -> Result<TaskCrd> {
        let api = self.api_namespaced::<TaskCrd>();
        let task = api.get(name).await?;

        match &task.status {
            Some(status) if status.state == TaskState::Ready => match &status.spec {
                Some(_) => Ok(task),
                None => bail!("task has no spec status: {name:?}"),
            },
            Some(_) | None => bail!("task is not ready: {name:?}"),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_task_all(&self) -> Result<Vec<ResourceRef>> {
        let api = self.api_namespaced::<TaskCrd>();
        let lp = ListParams::default();
        let tasks = api.list(&lp).await?;

        Ok(tasks
            .into_iter()
            .filter(|task| {
                task.status()
                    .map(|status| matches!(status.state, TaskState::Ready) && status.spec.is_some())
                    .unwrap_or_default()
            })
            .map(|task| ResourceRef {
                name: task.name_any(),
                namespace: task.namespace().unwrap(),
            })
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn load_task_all_by_model(&self, model_name: &str) -> Result<Vec<TaskCrd>> {
        let api = self.api_namespaced::<TaskCrd>();
        let lp = ListParams::default();
        let tasks = api.list(&lp).await?;

        Ok(tasks
            .into_iter()
            .filter(|task| {
                task.status()
                    .map(|status| matches!(status.state, TaskState::Ready) && status.spec.is_some())
                    .unwrap_or_default()
            })
            .filter(|task| {
                task.labels()
                    .get(Self::LABEL_SUBJECT)
                    .map(|name| name == model_name)
                    .unwrap_or_default()
            })
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    name: String,
    namespace: String,
}

fn convert_model_item(item: DynamicObject, parsed: &ModelFieldsNativeSpec) -> Result<Value> {
    let mut template = ItemTemplate::new_empty(parsed);
    template.update_field_value(InputFieldValue {
        name: "/".to_string(),
        value: ::serde_json::to_value(item)?,
    })?;
    template.finalize()
}
