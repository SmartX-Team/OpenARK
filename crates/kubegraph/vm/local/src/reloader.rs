use std::{fmt, marker::PhantomData, time::Duration};

use anyhow::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::{
    runtime::watcher::{watcher, Config, Error, Event},
    Api, Client, CustomResourceExt, Resource, ResourceExt,
};
use kubegraph_api::{
    graph::GraphScope,
    resource::{NetworkResource, NetworkResourceDB},
    vm::NetworkVirtualMachine,
};
use serde::de::DeserializeOwned;
use tokio::{task::JoinHandle, time::sleep};
use tracing::{error, info, instrument, warn, Level};

pub(crate) struct NetworkResourceReloader<K> {
    _crd: PhantomData<K>,
    inner: JoinHandle<()>,
}

impl<K> NetworkResourceReloader<K>
where
    K: 'static
        + Send
        + Clone
        + fmt::Debug
        + DeserializeOwned
        + Resource
        + CustomResourceExt
        + NetworkResource,
    <K as Resource>::DynamicType: Default,
{
    pub(crate) fn spawn<VM>(kube: Client, vm: &VM) -> Self
    where
        VM: NetworkVirtualMachine,
        <VM as NetworkVirtualMachine>::ResourceDB: NetworkResourceDB<K>,
    {
        Self {
            _crd: PhantomData,
            inner: ::tokio::spawn(loop_forever::<K>(
                kube,
                vm.resource_db().clone(),
                vm.fallback_interval(),
            )),
        }
    }

    pub(crate) fn abort(&self) {
        let name = <K as CustomResourceExt>::crd_name();
        info!("Stopping {name} reloader...");

        self.inner.abort()
    }
}

async fn loop_forever<K>(
    kube: Client,
    resource_db: impl 'static + NetworkResourceDB<K>,
    fallback_interval: Duration,
) where
    K: 'static
        + Send
        + Clone
        + fmt::Debug
        + DeserializeOwned
        + Resource
        + CustomResourceExt
        + NetworkResource,
    <K as Resource>::DynamicType: Default,
{
    let name = <K as CustomResourceExt>::crd_name();

    loop {
        if let Err(error) = try_loop_forever::<K>(&kube, &resource_db).await {
            error!("failed to operate {name} reloader: {error}");

            warn!("restaring {name} reloader in {fallback_interval:?}...");
            sleep(fallback_interval).await
        }
    }
}

async fn try_loop_forever<K>(
    kube: &Client,
    resource_db: &(impl 'static + NetworkResourceDB<K>),
) -> Result<()>
where
    K: 'static + Send + Clone + fmt::Debug + DeserializeOwned + Resource + NetworkResource,
    <K as Resource>::DynamicType: Default,
{
    let name = <K as CustomResourceExt>::crd_name();
    info!("Starting {name} reloader...");

    let default_namespace = kube.default_namespace().to_string();
    let default_namespace = || default_namespace.clone();
    let handle_event = |e| handle_event(resource_db, default_namespace, e);

    let api = Api::<K>::all(kube.clone());
    watcher(api, Config::default())
        .try_for_each(handle_event)
        .await
        .map_err(Into::into)
}

#[instrument(level = Level::INFO, skip(resource_db, default_namespace, event))]
async fn handle_event<K>(
    resource_db: &(impl 'static + NetworkResourceDB<K>),
    default_namespace: impl Copy + Fn() -> String,
    event: Event<K>,
) -> Result<(), Error>
where
    K: ResourceExt + NetworkResource,
{
    match event {
        Event::Applied(object) => handle_apply(resource_db, default_namespace, object).await,
        Event::Deleted(object) => handle_delete(resource_db, default_namespace, object).await,
        Event::Restarted(objects) => {
            objects
                .into_iter()
                .map(|object| handle_apply(resource_db, default_namespace, object))
                .collect::<FuturesUnordered<_>>()
                .try_collect()
                .await
        }
    }
}

#[instrument(level = Level::INFO, skip(resource_db, default_namespace, object))]
async fn handle_apply<K>(
    resource_db: &(impl 'static + NetworkResourceDB<K>),
    default_namespace: impl Fn() -> String,
    object: K,
) -> Result<(), Error>
where
    K: ResourceExt + NetworkResource,
{
    let namespace = object.namespace().unwrap_or_else(default_namespace);
    let name = object.name_any();
    let desc = object.description();

    info!("Applying {desc} connector: {namespace}/{name}");
    resource_db.insert(object).await;
    Ok(())
}

#[instrument(level = Level::INFO, skip(resource_db, default_namespace, object))]
async fn handle_delete<K>(
    resource_db: &(impl 'static + NetworkResourceDB<K>),
    default_namespace: impl Fn() -> String,
    object: K,
) -> Result<(), Error>
where
    K: ResourceExt + NetworkResource,
{
    let namespace = object.namespace().unwrap_or_else(default_namespace);
    let name = object.name_any();
    let desc = object.description();

    info!("Deleting {desc} connector: {namespace}/{name}");
    let scope = GraphScope { namespace, name };
    resource_db.delete(&scope).await;
    Ok(())
}
