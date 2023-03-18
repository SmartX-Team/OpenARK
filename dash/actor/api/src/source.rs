use dash_api::function::FunctionActorSourceConfigMapRefSpec;
use ipis::core::anyhow::{bail, Result};
use kiss_api::{
    k8s_openapi::api::core::v1::ConfigMap,
    kube::{Api, Client},
};

pub struct SourceClient<'a> {
    pub kube: &'a Client,
}

impl<'a> SourceClient<'a> {
    pub async fn load_config_map(
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
}
