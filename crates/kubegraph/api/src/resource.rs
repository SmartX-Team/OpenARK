use async_trait::async_trait;
use kube::Client;

use crate::{
    connector::NetworkConnectorCrd, function::NetworkFunctionCrd, graph::GraphScope,
    problem::NetworkProblemCrd,
};

#[async_trait]
pub trait NetworkResourceCollectionDB<T>
where
    Self: Sync
        + NetworkResourceClient
        + NetworkResourceDB<NetworkConnectorCrd>
        + NetworkResourceDB<NetworkFunctionCrd>
        + NetworkResourceDB<NetworkProblemCrd>,
{
}

#[async_trait]
impl<DB, T> NetworkResourceCollectionDB<T> for DB where
    Self: Sync
        + NetworkResourceClient
        + NetworkResourceDB<NetworkConnectorCrd>
        + NetworkResourceDB<NetworkFunctionCrd>
        + NetworkResourceDB<NetworkProblemCrd>
{
}

pub trait NetworkResourceClient {
    fn kube(&self) -> &Client;
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

pub trait NetworkResource {
    type Filter;

    fn description(&self) -> String;

    fn type_name() -> &'static str
    where
        Self: Sized;
}
