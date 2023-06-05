use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use dash_api::job::{DashJobCrd, DashJobState, DashJobStatus};
use dash_provider::storage::KubernetesStorageClient;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use log::{info, warn};
use serde_json::json;

use crate::validator::job::DashJobValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = DashJobCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::dash_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let namespace = data.namespace().unwrap();

        let validator = DashJobValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: &namespace,
                kube: &manager.kube,
            },
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            DashJobState::Pending => match validator.create(data.as_ref().clone()).await {
                Ok(()) => {
                    Self::update_spec_or_requeue(
                        &namespace,
                        &manager.kube,
                        &name,
                        DashJobState::Running,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to spawn dash jobs: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
            DashJobState::Running => match validator.is_running(data.as_ref().clone()).await {
                Ok(true) => Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                )),
                Ok(false) => match validator.delete(data.as_ref().clone()).await {
                    Ok(()) => {
                        Self::update_spec_or_requeue(
                            &namespace,
                            &manager.kube,
                            &name,
                            DashJobState::Completed,
                        )
                        .await
                    }
                    Err(e) => {
                        warn!("failed to delete dash job: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                        ))
                    }
                },
                Err(e) => {
                    warn!("failed to check dash job state: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
            DashJobState::Completed => Ok(Action::await_change()),
        }
    }
}

impl Ctx {
    async fn update_spec_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        state: DashJobState,
    ) -> Result<Action, Error> {
        match Self::update_spec(namespace, kube, name, state).await {
            Ok(()) => {
                info!("dash job is {state}: {name}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to update dash job state ({name:?} => {state}): {e}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
    }

    async fn update_spec(
        namespace: &str,
        kube: &Client,
        name: &str,
        state: DashJobState,
    ) -> Result<()> {
        let api = Api::<<Self as ::ark_core_k8s::manager::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::ark_core_k8s::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": DashJobStatus {
                state,
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
