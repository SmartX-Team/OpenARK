use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, StreamExt, TryStreamExt};
use kube::{
    runtime::watcher::{watcher, Config, Error, Event},
    Api, Client, ResourceExt,
};
use kubegraph_api::{
    connector::{NetworkConnectorCrd, NetworkConnectors},
    db::NetworkGraphDB,
};
use tokio::time::sleep;
use tracing::{error, info, warn};

pub async fn loop_forever(db: impl NetworkConnectors + NetworkGraphDB) {
    loop {
        if let Err(error) = try_loop_forever(&db).await {
            error!("failed to run http server: {error}");

            let duration = Duration::from_secs(5);
            warn!("restaring reloader in {duration:?}...");
            sleep(duration).await
        }
    }
}

async fn try_loop_forever(db: &(impl NetworkConnectors + NetworkGraphDB)) -> Result<()> {
    let client = Client::try_default()
        .await
        .map_err(|error| anyhow!("failed to load kubernetes account: {error}"))?;

    let default_namespace = client.default_namespace().to_string();
    let default_namespace = || default_namespace.clone();
    let handle_event = |e| handle_event(db, default_namespace, e);

    let api = Api::<NetworkConnectorCrd>::all(client);
    watcher(api, Config::default())
        .try_for_each(handle_event)
        .await
        .map_err(Into::into)
}

async fn handle_event(
    db: &(impl NetworkConnectors + NetworkGraphDB),
    default_namespace: impl Copy + Fn() -> String,
    event: Event<NetworkConnectorCrd>,
) -> Result<(), Error> {
    match event {
        Event::Applied(object) => handle_apply(db, default_namespace, object).await,
        Event::Deleted(object) => handle_delete(db, default_namespace, object).await,
        Event::Restarted(objects) => {
            ::futures::stream::iter(objects)
                .map(|object| handle_apply(db, default_namespace, object))
                .collect::<FuturesUnordered<_>>()
                .await
                .try_collect()
                .await
        }
    }
}

async fn handle_apply(
    db: &(impl NetworkConnectors + NetworkGraphDB),
    default_namespace: impl Fn() -> String,
    object: NetworkConnectorCrd,
) -> Result<(), Error> {
    let name = object.name_any();
    let namespace = object.namespace().unwrap_or_else(default_namespace);
    let r#type = object.spec.name();

    info!("Applying {type} connector: {namespace}/{name}");
    db.add_connector(object).await;
    Ok(())
}

async fn handle_delete(
    db: &(impl NetworkConnectors + NetworkGraphDB),
    default_namespace: impl Fn() -> String,
    object: NetworkConnectorCrd,
) -> Result<(), Error> {
    let name = object.name_any();
    let namespace = object.namespace().unwrap_or_else(default_namespace);
    let r#type = object.spec.name();

    info!("Deleting {type} connector: {namespace}/{name}");
    db.delete_connector(namespace, name).await;
    Ok(())
}
