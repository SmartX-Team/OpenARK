use anyhow::{bail, Result};
use dash_api::{
    function::{FunctionActorSourceConfigMapRefSpec, FunctionCrd, FunctionState},
    model::{ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldsNativeSpec, ModelState},
    model_storage_binding::{ModelStorageBindingCrd, ModelStorageBindingState},
    storage::{ModelStorageCrd, ModelStorageSpec, ModelStorageState},
};
use itertools::Itertools;
use k8s_openapi::{
    api::core::v1::ConfigMap,
    apiextensions_apiserver::pkg::apis::apiextensions::v1::{
        CustomResourceDefinition, CustomResourceDefinitionVersion,
    },
};
use kube::{
    api::ListParams,
    core::{object::HasStatus, DynamicObject},
    discovery, Api, Client, ResourceExt,
};
use serde_json::Value;

use crate::input::{InputFieldValue, ItemTemplate};

#[derive(Copy, Clone)]
pub struct KubernetesStorageClient<'a> {
    pub kube: &'a Client,
}

impl<'a> KubernetesStorageClient<'a> {
    const LABEL_SUBJECT: &'static str = "dash.ulagbulag.io/subject";

    pub async fn load_config_map<'f>(
        &self,
        spec: &'f FunctionActorSourceConfigMapRefSpec,
    ) -> Result<(&'f str, String)> {
        let FunctionActorSourceConfigMapRefSpec {
            name,
            namespace,
            path,
        } = spec;

        let api = Api::<ConfigMap>::namespaced(self.kube.clone(), namespace);
        let config_map = api.get(name).await?;

        match config_map.data.and_then(|mut data| data.remove(path)) {
            Some(content) => Ok((path, content)),
            None => bail!("no such file in ConfigMap: {path:?} in {namespace}::{name}"),
        }
    }

    pub async fn load_custom_resource(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        parsed: &ModelFieldsNativeSpec,
        namespace: &str,
        resource_name: &str,
    ) -> Result<Option<Value>> {
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
        let api: Api<DynamicObject> = match scope.as_str() {
            "Namespaced" => Api::namespaced_with(self.kube.clone(), namespace, &ar),
            "Cluster" => Api::all_with(self.kube.clone(), &ar),
            scope => bail!("cannot infer CRD scope {scope:?}: {resource_name:?}"),
        };
        api.get_opt(resource_name)
            .await?
            .map(|item| convert_model_item(item, parsed))
            .transpose()
    }

    pub async fn load_custom_resource_all(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        parsed: &ModelFieldsNativeSpec,
        namespace: &str,
    ) -> Result<Vec<Value>> {
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
        let api: Api<DynamicObject> = match scope.as_str() {
            "Namespaced" => Api::namespaced_with(self.kube.clone(), namespace, &ar),
            "Cluster" => Api::all_with(self.kube.clone(), &ar),
            scope => bail!("cannot infer CRD scope {scope:?}"),
        };
        let lp = ListParams::default();
        api.list(&lp).await.map_err(Into::into).and_then(|list| {
            list.items
                .into_iter()
                .map(|item| convert_model_item(item, parsed))
                .collect()
        })
    }

    pub async fn load_custom_resource_definition(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
    ) -> Result<(String, String, CustomResourceDefinitionVersion)> {
        let (api_group, version) = crate::imp::parse_api_version(&spec.name)?;

        let api = Api::<CustomResourceDefinition>::all(self.kube.clone());
        let crd = api.get(api_group).await?;

        match crd.spec.versions.iter().find(|def| def.name == version) {
            Some(def) => Ok((crd.spec.group, crd.spec.scope, def.clone())),
            None => bail!(
                "CRD version is invalid; expected one of {:?}, but given {version}",
                crd.spec.versions.iter().map(|def| &def.name).join(","),
            ),
        }
    }

    pub async fn load_model(&self, name: &str) -> Result<ModelCrd> {
        let api = Api::<ModelCrd>::all(self.kube.clone());
        let model = api.get(name).await?;

        match &model.status {
            Some(status) if status.state == ModelState::Ready => match &status.fields {
                Some(_) => Ok(model),
                None => bail!("model has no fields status: {name:?}"),
            },
            Some(_) | None => bail!("model is not ready: {name:?}"),
        }
    }

    pub async fn load_model_all(&self) -> Result<Vec<String>> {
        let api = Api::<ModelCrd>::all(self.kube.clone());
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
            .map(|model| model.name_any())
            .collect())
    }

    pub async fn load_model_storage(&self, name: &str) -> Result<ModelStorageCrd> {
        let api = Api::<ModelStorageCrd>::all(self.kube.clone());
        let storage = api.get(name).await?;

        match &storage.status {
            Some(status) if status.state == ModelStorageState::Ready => Ok(storage),
            Some(_) | None => bail!("model storage is not ready: {name:?}"),
        }
    }

    pub async fn load_model_storage_bindings(
        &self,
        model_name: &str,
    ) -> Result<Vec<ModelStorageSpec>> {
        let api = Api::<ModelStorageBindingCrd>::all(self.kube.clone());
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
            .filter(|binding| binding.spec.model == model_name)
            .filter_map(|binding| binding.status.unwrap().storage)
            .collect())
    }

    pub async fn load_function(&self, name: &str) -> Result<FunctionCrd> {
        let api = Api::<FunctionCrd>::all(self.kube.clone());
        let function = api.get(name).await?;

        match &function.status {
            Some(status) if status.state == FunctionState::Ready => match &status.spec {
                Some(_) => Ok(function),
                None => bail!("function has no spec status: {name:?}"),
            },
            Some(_) | None => bail!("function is not ready: {name:?}"),
        }
    }

    pub async fn load_function_all(&self) -> Result<Vec<String>> {
        let api = Api::<FunctionCrd>::all(self.kube.clone());
        let lp = ListParams::default();
        let functions = api.list(&lp).await?;

        Ok(functions
            .into_iter()
            .filter(|function| {
                function
                    .status()
                    .map(|status| {
                        matches!(status.state, FunctionState::Ready) && status.spec.is_some()
                    })
                    .unwrap_or_default()
            })
            .map(|function| function.name_any())
            .collect())
    }

    pub async fn load_function_all_by_model(&self, model_name: &str) -> Result<Vec<FunctionCrd>> {
        let api = Api::<FunctionCrd>::all(self.kube.clone());
        let lp = ListParams::default();
        let functions = api.list(&lp).await?;

        Ok(functions
            .into_iter()
            .filter(|function| {
                function
                    .status()
                    .map(|status| {
                        matches!(status.state, FunctionState::Ready) && status.spec.is_some()
                    })
                    .unwrap_or_default()
            })
            .filter(|function| {
                function
                    .labels()
                    .get(Self::LABEL_SUBJECT)
                    .map(|name| name == model_name)
                    .unwrap_or_default()
            })
            .collect())
    }
}

fn convert_model_item(item: DynamicObject, parsed: &ModelFieldsNativeSpec) -> Result<Value> {
    let mut template = ItemTemplate::new_empty(parsed);
    template.update_field_value(InputFieldValue {
        name: "/".to_string(),
        value: ::serde_json::to_value(item)?,
    })?;
    template.finalize()
}
