pub mod job;
pub mod package;

use std::fmt;

use ipis::core::anyhow::Result;
use kiss_api::{
    k8s_openapi::{
        serde::{de::DeserializeOwned, Serialize},
        NamespaceResourceScope,
    },
    kube::{Api, Client, Resource, ResourceExt, api::DeleteParams},
};

async fn try_delete<K>(kube: &Client, namespace: &str, name: &str) -> Result<()>
where
    K: Clone
        + fmt::Debug
        + Serialize
        + DeserializeOwned
        + Resource<Scope = NamespaceResourceScope>
        + ResourceExt,
    <K as Resource>::DynamicType: Default,
{
    let api = Api::<K>::namespaced(kube.clone(), namespace);
    if api.get_opt(name).await?.is_some() {
        let dp = DeleteParams::default();
        api.delete(name, &dp).await.map(|_| ()).map_err(Into::into)
    } else {
        Ok(())
    }
}
