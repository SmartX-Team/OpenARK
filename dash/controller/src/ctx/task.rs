use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use dash_api::{
    model::ModelFieldKindNativeSpec,
    task::{TaskCrd, TaskSpec, TaskState, TaskStatus},
};
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use serde_json::json;
use tracing::{info, instrument, warn, Level};

use crate::validator::task::TaskValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = TaskCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::dash_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    #[instrument(level = Level::INFO, skip_all, fields(name = data.name_any(), namespace = data.namespace()), err(Display))]
    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let namespace = data.namespace().unwrap();

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            TaskState::Pending => {
                let validator = TaskValidator {
                    namespace: &namespace,
                    kube: &manager.kube,
                };
                match validator.validate_task(data.spec.clone()).await {
                    Ok(spec) => {
                        Self::update_spec_or_requeue(&namespace, &manager.kube, &name, spec).await
                    }
                    Err(e) => {
                        warn!("failed to validate task: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            TaskState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
        }
    }
}

impl Ctx {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn update_spec_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        spec: TaskSpec<ModelFieldKindNativeSpec>,
    ) -> Result<Action, Error> {
        match Self::update_spec(namespace, kube, name, spec).await {
            Ok(()) => {
                info!("task is ready: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to update task state ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube, spec), err(Display))]
    async fn update_spec(
        namespace: &str,
        kube: &Client,
        name: &str,
        spec: TaskSpec<ModelFieldKindNativeSpec>,
    ) -> Result<()> {
        let api = Api::<<Self as ::ark_core_k8s::manager::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::ark_core_k8s::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": TaskStatus {
                state: TaskState::Ready,
                spec: Some(spec),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
