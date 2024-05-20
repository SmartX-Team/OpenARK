use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::{
    runtime::watcher::{watcher, Config, Error, Event},
    Api, Client, ResourceExt,
};
use kubegraph_api::{
    connector::{NetworkConnectorCrd, NetworkConnectorDB},
    graph::GraphScope,
    vm::NetworkVirtualMachine,
};
use tokio::time::sleep;
use tracing::{error, info, warn};

pub async fn loop_forever(vm: impl NetworkVirtualMachine) {
    loop {
        if let Err(error) = try_loop_forever(&vm).await {
            error!("failed to run http server: {error}");

            let duration = Duration::from_secs(5);
            warn!("restaring reloader in {duration:?}...");
            sleep(duration).await
        }
    }
}

async fn try_loop_forever(vm: &impl NetworkVirtualMachine) -> Result<()> {
    let client = Client::try_default()
        .await
        .map_err(|error| anyhow!("failed to load kubernetes account: {error}"))?;

    let default_namespace = client.default_namespace().to_string();
    let default_namespace = || default_namespace.clone();
    let handle_event = |e| handle_event(vm, default_namespace, e);

    let api = Api::<NetworkConnectorCrd>::all(client);
    watcher(api, Config::default())
        .try_for_each(handle_event)
        .await
        .map_err(Into::into)
}

async fn handle_event(
    vm: &impl NetworkVirtualMachine,
    default_namespace: impl Copy + Fn() -> String,
    event: Event<NetworkConnectorCrd>,
) -> Result<(), Error> {
    match event {
        Event::Applied(object) => handle_apply(vm, default_namespace, object).await,
        Event::Deleted(object) => handle_delete(vm, default_namespace, object).await,
        Event::Restarted(objects) => {
            objects
                .into_iter()
                .map(|object| handle_apply(vm, default_namespace, object))
                .collect::<FuturesUnordered<_>>()
                .try_collect()
                .await
        }
    }
}

async fn handle_apply(
    vm: &impl NetworkVirtualMachine,
    default_namespace: impl Fn() -> String,
    object: NetworkConnectorCrd,
) -> Result<(), Error> {
    let name = object.name_any();
    let namespace = object.namespace().unwrap_or_else(default_namespace);
    let r#type = object.spec.name();

    info!("Applying {type} connector: {namespace}/{name}");
    vm.resource_db().insert_connector(object).await;
    Ok(())
}

async fn handle_delete(
    vm: &impl NetworkVirtualMachine,
    default_namespace: impl Fn() -> String,
    object: NetworkConnectorCrd,
) -> Result<(), Error> {
    let name = object.name_any();
    let namespace = object.namespace().unwrap_or_else(default_namespace);
    let r#type = object.spec.name();

    info!("Deleting {type} connector: {namespace}/{name}");
    let scope = GraphScope { namespace, name };
    vm.resource_db().delete_connector(&scope).await;
    Ok(())
}
