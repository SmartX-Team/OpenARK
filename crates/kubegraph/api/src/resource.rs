use async_trait::async_trait;
use kube::CustomResourceExt;

use crate::{
    connector::NetworkConnectorCrd, function::NetworkFunctionCrd, graph::GraphScope,
    problem::NetworkProblemCrd,
};

#[async_trait]
pub trait NetworkResourceCollectionDB
where
    Self: Sync
        + NetworkResourceDB<NetworkConnectorCrd>
        + NetworkResourceDB<NetworkFunctionCrd>
        + NetworkResourceDB<NetworkProblemCrd>,
{
}

#[async_trait]
impl<T> NetworkResourceCollectionDB for T where
    Self: Sync
        + NetworkResourceDB<NetworkConnectorCrd>
        + NetworkResourceDB<NetworkFunctionCrd>
        + NetworkResourceDB<NetworkProblemCrd>
{
}

#[async_trait]
pub trait NetworkResourceDB<K>
where
    K: NetworkResource,
{
    async fn delete(&self, key: &GraphScope);

    async fn insert(&self, object: K);

    async fn list(&self, filter: <K as NetworkResource>::Filter) -> Option<Vec<K>>;
}

pub trait NetworkResource
where
    Self: CustomResourceExt,
{
    type Filter;

    fn description(&self) -> String {
        <Self as CustomResourceExt>::crd_name().into()
    }
}
