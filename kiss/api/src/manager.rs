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
    api::{ListParams, Patch, PatchParams, PostParams},
    runtime::{controller::Action, Controller},
    Api, Client, CustomResourceExt, Error, Resource, ResourceExt,
};
use serde::de::DeserializeOwned;

pub struct Manager<C> {
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

    const NAME: &'static str;
    const NAMESPACE: Option<&'static str> = None;
    const FALLBACK: Duration = Duration::from_secs(30 * 60); // 30 minutes

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        Default::default()
    }

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
        let create_crd = |api: Api<CustomResourceDefinition>, crd: CustomResourceDefinition| async move {
            let name = crd.name_any();
            if api.get_opt(&name).await?.is_none() {
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some(<Self as Ctx>::NAME.into()),
                };
                api.create(&pp, &crd).await?;

                info!("Created CRD: {name}");
                Result::<_, Error>::Ok(())
            } else {
                let pp = PatchParams {
                    dry_run: false,
                    force: true,
                    field_manager: Some(<Self as Ctx>::NAME.into()),
                    ..Default::default()
                };
                api.patch(&name, &pp, &Patch::Apply(&crd)).await?;

                info!("Updated CRD: {name}");
                Result::<_, Error>::Ok(())
            }
        };

        logger::init_once();
        <Self as Ctx>::try_spawn(|_, client| async move {
            // Ensure CRD is installed before loop-watching
            let api = Api::<CustomResourceDefinition>::all(client);

            for crd in <Self as Ctx>::get_subcrds() {
                create_crd(api.clone(), crd).await?;
            }
            create_crd(api, <Self as Ctx>::Data::crd()).await?;
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
