use std::{marker::PhantomData, sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::{Ctx, Manager};
use async_trait::async_trait;
use futures::TryFutureExt;
use k8s_openapi::api::core::v1::Namespace;
use kube::{runtime::controller::Action, Error, ResourceExt};
use tracing::{info, instrument, warn, Level};

use crate::validator::injector::InjectorValidator;

macro_rules! define_injector {
    [ $( $kind:ident ),* ] => {
        $(
            pub mod $kind {
                pub type Ctx = super::InjectionCtx<InjectionCtxParams>;

                #[derive(Default)]
                pub struct InjectionCtxParams;

                impl super::InjectionCtxParams for InjectionCtxParams {
                    const CONTENT: &'static str = include_str!(concat!("./", stringify!($kind), ".yaml.j2"));
                    const KIND: &'static str = stringify!($kind);
                }
            }
        )*
    };
}

define_injector![otlp];

#[derive(Default)]
pub struct InjectionCtx<P>(PhantomData<P>)
where
    P: InjectionCtxParams;

pub trait InjectionCtxParams
where
    Self: 'static + Default + Send + Sync,
{
    const CONTENT: &'static str;
    const KIND: &'static str;
}

#[async_trait]
impl<P> Ctx for InjectionCtx<P>
where
    P: InjectionCtxParams,
{
    type Data = Namespace;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::dash_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    #[instrument(level = Level::INFO, skip_all, fields(name = data.name_any()), err(Display))]
    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let kind = <P as InjectionCtxParams>::KIND;
        let label = format!("dash.ulagbulag.io/inject-{kind}");

        let validator = InjectorValidator {
            content: <P as InjectionCtxParams>::CONTENT,
            namespace: &name,
            kube: &manager.kube,
        };

        match data
            .labels()
            .get(&label)
            .and_then(|value| value.parse().ok())
        {
            Some(true) => match validator
                .exists()
                .and_then(|exists| {
                    let name = name.clone();
                    async move {
                        if !exists {
                            validator.create().await.map(|()| {
                                info!("created {kind} collector: {name:?}");
                            })
                        } else {
                            info!("skipped creating {kind} collector: {name:?}: already created");
                            Ok(())
                        }
                    }
                })
                .await
            {
                Ok(()) => Ok(Action::await_change()),
                Err(e) => {
                    warn!("failed to create {kind} collector: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
            Some(false) | None => match validator
                .exists()
                .and_then(|exists| {
                    let name = name.clone();
                    async move {
                        if exists {
                            validator.delete().await.map(|()| {
                                info!("deleted {kind} collector: {name:?}");
                            })
                        } else {
                            info!("skipped deleting {kind} collector: {name:?}: not labeled (feature flag is off)");
                            Ok(())
                        }
                    }
                })
                .await
            {
                Ok(()) => Ok(Action::await_change()),
                Err(e) => {
                    warn!("failed to delete {kind} collector: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
        }
    }
}
