use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, StreamExt, TryStreamExt};
use kube::{
    runtime::watcher::{watcher, Config, Error, Event},
    Api, Client, ResourceExt,
};
use kubegraph_api::{connector::NetworkConnectorCrd, provider::NetworkGraphProvider};
use tokio::time::sleep;
use tracing::{error, info, warn};

pub async fn loop_forever(graph: impl NetworkGraphProvider) {
    loop {
        if let Err(error) = try_loop_forever(&graph).await {
            error!("failed to run http server: {error}");

            let duration = Duration::from_secs(5);
            warn!("restaring reloader in {duration:?}...");
            sleep(duration).await
        }
    }
}

async fn try_loop_forever(graph: &impl NetworkGraphProvider) -> Result<()> {
    let client = Client::try_default()
        .await
        .map_err(|error| anyhow!("failed to load kubernetes account: {error}"))?;

    let default_namespace = client.default_namespace().to_string();
    let default_namespace = || default_namespace.clone();
    let handle_event = |e| handle_event(graph, default_namespace, e);

    let api = Api::<NetworkConnectorCrd>::all(client);
    watcher(api, Config::default())
        .try_for_each(handle_event)
        .await
        .map_err(Into::into)
}

async fn handle_event(
    graph: &impl NetworkGraphProvider,
    default_namespace: impl Copy + Fn() -> String,
    event: Event<NetworkConnectorCrd>,
) -> Result<(), Error> {
    match event {
        Event::Applied(object) => handle_apply(graph, default_namespace, object).await,
        Event::Deleted(object) => handle_delete(graph, default_namespace, object).await,
        Event::Restarted(objects) => {
            ::futures::stream::iter(objects)
                .map(|object| handle_apply(graph, default_namespace, object))
                .collect::<FuturesUnordered<_>>()
                .await
                .try_collect()
                .await
        }
    }
}

async fn handle_apply(
    graph: &impl NetworkGraphProvider,
    default_namespace: impl Fn() -> String,
    object: NetworkConnectorCrd,
) -> Result<(), Error> {
    let name = object.name_any();
    let namespace = object.namespace().unwrap_or_else(default_namespace);

    info!("Applying network connector: {namespace}/{name}");
    graph.add_connector(namespace, name, object.spec).await;
    Ok(())
}

async fn handle_delete(
    graph: &impl NetworkGraphProvider,
    default_namespace: impl Fn() -> String,
    object: NetworkConnectorCrd,
) -> Result<(), Error> {
    let name = object.name_any();
    let namespace = object.namespace().unwrap_or_else(default_namespace);

    info!("Deleting network connector: {namespace}/{name}");
    graph.delete_connector(namespace, name).await;
    Ok(())
}
