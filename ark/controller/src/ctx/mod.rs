pub mod job;
pub mod package;

use std::fmt;

use ipis::core::anyhow::Result;
use kiss_api::{
    k8s_openapi::{
        serde::{de::DeserializeOwned, Serialize},
        NamespaceResourceScope,
    },
    kube::{
        api::{DeleteParams, ListParams},
        Api, Client, Resource, ResourceExt,
    },
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

async fn try_delete_all<K>(kube: &Client, namespace: &str, lp: &ListParams) -> Result<()>
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
    let dp = DeleteParams::default();
    api.delete_collection(&dp, lp)
        .await
        .map(|_| ())
        .map_err(Into::into)
}
