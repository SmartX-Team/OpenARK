use dash_api::{
    function::{FunctionActorSourceConfigMapRefSpec, FunctionCrd, FunctionSpec, FunctionState},
    model::{
        ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldKindNativeSpec,
        ModelFieldKindSpec, ModelFieldsNativeSpec, ModelSpec, ModelState,
    },
};
use ipis::{
    core::anyhow::{bail, Result},
    itertools::Itertools,
};
use kiss_api::{
    k8s_openapi::{
        api::core::v1::ConfigMap,
        apiextensions_apiserver::pkg::apis::apiextensions::v1::{
            CustomResourceDefinition, CustomResourceDefinitionVersion,
        },
    },
    kube::{core::DynamicObject, discovery, Api, Client},
    serde_json::Value,
};

pub struct SourceClient<'a> {
    pub kube: &'a Client,
}

impl<'a> SourceClient<'a> {
    pub async fn load_kube_config_map(
        &self,
        spec: FunctionActorSourceConfigMapRefSpec,
    ) -> Result<(String, String)> {
        let FunctionActorSourceConfigMapRefSpec {
            name,
            namespace,
            path,
        } = spec;

        let api = Api::<ConfigMap>::namespaced(self.kube.clone(), &namespace);
        let config_map = api.get(&name).await?;

        match config_map.data.and_then(|mut data| data.remove(&path)) {
            Some(content) => Ok((path, content)),
            None => bail!("no such file in ConfigMap: {path:?} in {namespace}::{name}"),
        }
    }

    pub async fn load_kube_custom_resource(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
        namespace: Option<&str>,
        resource_name: &str,
    ) -> Result<Value> {
        let (api_group, def) = self.load_custom_resource_definition(spec).await?;

        // Discover most stable version variant of document
        let apigroup = discovery::group(self.kube, &api_group).await?;
        let ar = match apigroup.versioned_resources(&def.name).pop() {
            Some((ar, _)) => ar,
            None => {
                let model_name = &spec.name;
                bail!("no such CRD: {model_name:?}")
            }
        };

        // Use the discovered kind in an Api, and Controller with the ApiResource as its DynamicType
        let api: Api<DynamicObject> = match namespace {
            Some(namespace) => Api::namespaced_with(self.kube.clone(), namespace, &ar),
            None => Api::all_with(self.kube.clone(), &ar),
        };
        Ok(api.get(resource_name).await?.data)
    }

    pub async fn load_custom_resource_definition(
        &self,
        spec: &ModelCustomResourceDefinitionRefSpec,
    ) -> Result<(String, CustomResourceDefinitionVersion)> {
        let (api_group, version) = crate::imp::parse_api_version(&spec.name)?;

        let api = Api::<CustomResourceDefinition>::all(self.kube.clone());
        let crd = api.get(api_group).await?;

        match crd.spec.versions.iter().find(|def| def.name == version) {
            Some(def) => Ok((crd.spec.group, def.clone())),
            None => bail!(
                "CRD version is invalid; expected one of {:?}, but given {version}",
                crd.spec.versions.iter().map(|def| &def.name).join(","),
            ),
        }
    }

    pub async fn load_function(
        &self,
        name: &str,
    ) -> Result<(
        FunctionSpec<ModelFieldKindSpec>,
        FunctionSpec<ModelFieldKindNativeSpec>,
    )> {
        let api = Api::<FunctionCrd>::all(self.kube.clone());
        let function = api.get(name).await?;

        match function.status {
            Some(status) if status.state == Some(FunctionState::Ready) => match status.spec {
                Some(spec) => Ok((function.spec, spec)),
                None => bail!("function has no spec status: {name:?}"),
            },
            Some(_) | None => bail!("function is not ready: {name:?}"),
        }
    }

    pub async fn load_model(&self, name: &str) -> Result<(ModelSpec, ModelFieldsNativeSpec)> {
        let api = Api::<ModelCrd>::all(self.kube.clone());
        let model = api.get(name).await?;

        match model.status {
            Some(status) if status.state == Some(ModelState::Ready) => match status.fields {
                Some(parsed) => Ok((model.spec, parsed)),
                None => bail!("model has no fields status: {name:?}"),
            },
            Some(_) | None => bail!("model is not ready: {name:?}"),
        }
    }
}
