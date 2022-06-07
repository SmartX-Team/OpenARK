use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::anyhow::Result,
    futures::{self, StreamExt},
    log::{info, warn},
    logger,
    tokio::sync::RwLock,
};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{ListParams, PostParams},
    runtime::{controller::Action, Controller},
    Api, Client, CustomResourceExt, Error, Resource, ResourceExt,
};
use serde::de::DeserializeOwned;

pub struct Manager<C> {
    pub client: Client,
    pub ctx: Arc<RwLock<C>>,
}

#[async_trait]
pub trait Ctx
where
    Self: 'static + Send + Sync + Default,
    <Self as Ctx>::Data:
        Send + Sync + Clone + ::core::fmt::Debug + DeserializeOwned + CustomResourceExt + Resource,
    <<Self as Ctx>::Data as Resource>::DynamicType:
        Clone + ::core::fmt::Debug + Default + Eq + Unpin + ::core::hash::Hash,
{
    type Data;

    async fn spawn() {
        logger::init_once();
        <Self as Ctx>::try_spawn()
            .await
            .expect("spawning a manager")
    }

    async fn try_spawn() -> Result<()> {
        let client = Client::try_default().await?;
        let ctx = Arc::new(RwLock::new(Self::default()));
        let manager = Arc::new(Manager {
            client: client.clone(),
            ctx: ctx.clone(),
        });

        let api = Api::<<Self as Ctx>::Data>::all(client.clone());
        // Ensure CRD is installed before loop-watching
        if api.list(&ListParams::default().limit(1)).await.is_err() {
            let crd = <Self as Ctx>::Data::crd();
            let name = crd.name();
            let api = Api::<CustomResourceDefinition>::all(client);

            let pp = PostParams {
                dry_run: false,
                field_manager: Some("kube-controller".into()),
            };
            api.create(&pp, &crd).await?;

            info!("Created CRD: {name}");
        }

        // All good. Start controller and return its future.
        Controller::new(api, ListParams::default())
            .run(
                |data, manager| Self::reconcile(manager, data),
                |error, manager| {
                    warn!("failed to reconcile: {:?}", error);
                    Self::error_policy(manager, error)
                },
                manager,
            )
            .for_each(|_| futures::future::ready(()))
            .await;
        Ok(())
    }

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized;

    fn error_policy<E>(_manager: Arc<Manager<Self>>, _error: E) -> Action
    where
        Self: Sized,
        E: ::std::fmt::Debug,
    {
        Action::await_change()
    }
}
