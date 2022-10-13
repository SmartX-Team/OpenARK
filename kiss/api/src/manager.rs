use core::{future::Future, time::Duration};
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

use crate::{ansible::AnsibleClient, cluster::ClusterManager};

pub struct Manager<C> {
    pub ansible: AnsibleClient,
    pub cluster: ClusterManager,
    pub kube: Client,
    pub ctx: Arc<RwLock<C>>,
}

#[async_trait]
pub trait Ctx
where
    Self: 'static + Send + Sync + Default,
    <Self as Ctx>::Data: Send + Sync + Clone + ::core::fmt::Debug + DeserializeOwned + Resource,
    <<Self as Ctx>::Data as Resource>::DynamicType:
        Clone + ::core::fmt::Debug + Default + Eq + Unpin + ::core::hash::Hash,
{
    type Data;

    const NAMESPACE: Option<&'static str> = None;
    const FALLBACK: Duration = Duration::from_secs(30 * 60); // 30 minutes

    async fn spawn() {
        logger::init_once();
        <Self as Ctx>::try_spawn(|_, _| async move { Ok(()) })
            .await
            .expect("spawning a manager")
    }

    async fn spawn_crd()
    where
        <Self as Ctx>::Data: CustomResourceExt,
    {
        logger::init_once();
        <Self as Ctx>::try_spawn(|api, client| async move {
            // Ensure CRD is installed before loop-watching
            if api.list(&ListParams::default().limit(1)).await.is_err() {
                let crd = <Self as Ctx>::Data::crd();
                let name = crd.name_any();
                let api = Api::<CustomResourceDefinition>::all(client);

                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some("kube-controller".into()),
                };
                api.create(&pp, &crd).await?;

                info!("Created CRD: {name}");
            }
            Ok(())
        })
        .await
        .expect("spawning a manager with CRD")
    }

    async fn try_spawn<F, Fut>(f_init: F) -> Result<()>
    where
        F: FnOnce(Api<<Self as Ctx>::Data>, Client) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        let client = Client::try_default().await?;
        let ctx = Arc::new(RwLock::new(Self::default()));
        let manager = Arc::new(Manager {
            ansible: AnsibleClient::try_default(&client).await?,
            cluster: Default::default(),
            kube: client.clone(),
            ctx: ctx.clone(),
        });

        let api = match Self::NAMESPACE {
            Some(ns) => Api::<<Self as Ctx>::Data>::namespaced(client.clone(), ns),
            None => Api::<<Self as Ctx>::Data>::all(client.clone()),
        };
        f_init(api.clone(), client).await?;

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
        Action::requeue(<Self as Ctx>::FALLBACK)
    }
}
