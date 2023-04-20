use core::{future::Future, time::Duration};
use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::anyhow::Result,
    futures::{self, StreamExt},
    log::{info, warn},
    logger,
};
use k8s_openapi::{
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
    NamespaceResourceScope,
};
use kube::{
    api::{Patch, PatchParams, PostParams},
    runtime::{controller::Action, watcher::Config, Controller},
    Api, Client, CustomResourceExt, Error, Resource, ResourceExt,
};
use serde::de::DeserializeOwned;

pub struct Manager<C> {
    pub kube: Client,
    pub ctx: Arc<C>,
}

#[async_trait]
pub trait Ctx
where
    Self: 'static + Send + Sync + TryDefault,
    <Self as Ctx>::Data:
        Send + Sync + Clone + ::core::fmt::Debug + DeserializeOwned + Resource<DynamicType = ()>,
    <<Self as Ctx>::Data as Resource>::DynamicType:
        Clone + ::core::fmt::Debug + Default + Eq + Unpin + ::core::hash::Hash,
{
    type Data;

    const NAME: &'static str;
    const NAMESPACE: &'static str;
    const FALLBACK: Duration = Duration::from_secs(30 * 60); // 30 minutes

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        Default::default()
    }

    async fn spawn()
    where
        Self: Sized,
    {
        <Self as Ctx>::try_spawn(|client| async move { Ok(Self::init_resource(client)) })
            .await
            .expect("spawning a manager with resource")
    }

    async fn spawn_namespaced()
    where
        Self: Sized,
        <Self as Ctx>::Data: Resource<Scope = NamespaceResourceScope>,
    {
        <Self as Ctx>::try_spawn(|client| async move { Ok(Self::init_resource_namespaced(client)) })
            .await
            .expect("spawning a manager with namespaced resource")
    }

    async fn spawn_crd()
    where
        Self: Sized,
        <Self as Ctx>::Data: CustomResourceExt,
    {
        <Self as Ctx>::try_spawn(|client| async move {
            Self::init_crd(client.clone())
                .await
                .map(|()| Self::init_resource(client))
        })
        .await
        .expect("spawning a manager with CRD")
    }

    async fn spawn_crd_namespaced()
    where
        Self: Sized,
        <Self as Ctx>::Data: CustomResourceExt + Resource<Scope = NamespaceResourceScope>,
    {
        <Self as Ctx>::try_spawn(|client| async move {
            Self::init_crd(client.clone())
                .await
                .map(|()| Self::init_resource_namespaced(client))
        })
        .await
        .expect("spawning a manager with namespaced CRD")
    }

    async fn try_spawn<F, Fut>(f_init: F) -> Result<()>
    where
        Self: Sized,
        F: FnOnce(Client) -> Fut + Send,
        Fut: Future<Output = Result<Api<<Self as Ctx>::Data>>> + Send,
    {
        logger::init_once();

        let client = Client::try_default().await?;
        let ctx = Arc::new(Self::try_default().await?);
        let manager = Arc::new(Manager {
            kube: client.clone(),
            ctx: ctx.clone(),
        });

        let api = f_init(client).await?;

        // All good. Start controller and return its future.
        Controller::new(api, Config::default())
            .run(
                |data, manager| Self::reconcile(manager, data),
                |data, error, manager| {
                    let kind = <<Self as Ctx>::Data>::kind(&());
                    let name = data.name_any();
                    warn!("failed to reconcile {kind} {name:?}: {error:?}");
                    Self::error_policy(manager, error)
                },
                manager,
            )
            .for_each(|_| futures::future::ready(()))
            .await;
        Ok(())
    }

    fn init_resource(client: Client) -> Api<<Self as Ctx>::Data> {
        Api::<<Self as Ctx>::Data>::all(client)
    }

    fn init_resource_namespaced(client: Client) -> Api<<Self as Ctx>::Data>
    where
        <Self as Ctx>::Data: Resource<Scope = NamespaceResourceScope>,
    {
        Api::<<Self as Ctx>::Data>::namespaced(client, <Self as Ctx>::NAMESPACE)
    }

    async fn init_crd(client: Client) -> Result<()>
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

        // Ensure CRD is installed before loop-watching
        let api = Api::<CustomResourceDefinition>::all(client);

        for crd in <Self as Ctx>::get_subcrds() {
            create_crd(api.clone(), crd).await?;
        }
        create_crd(api, <Self as Ctx>::Data::crd()).await?;
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

#[async_trait]
pub trait TryDefault {
    async fn try_default() -> Result<Self>
    where
        Self: Sized;
}

#[async_trait]
impl<T> TryDefault for T
where
    T: Default,
{
    async fn try_default() -> Result<Self> {
        Ok(T::default())
    }
}
