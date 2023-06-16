use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use dash_api::job::{DashJobCrd, DashJobState, DashJobStatus};
use dash_provider::storage::KubernetesStorageClient;
use dash_provider_api::FunctionChannel;
use kube::{
    api::{DeleteParams, Patch, PatchParams},
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

        let now = Utc::now();
        let completed_job_gc_timeout = ::chrono::Duration::minutes(20);

        let validator = DashJobValidator {
            kubernetes_storage: KubernetesStorageClient {
                namespace: &namespace,
                kube: &manager.kube,
            },
        };

        if data.metadata.deletion_timestamp.is_some()
            && data
                .status
                .as_ref()
                .map(|status| status.state != DashJobState::Deleting)
                .unwrap_or(true)
        {
            return Self::update_spec_or_requeue(
                &namespace,
                &manager.kube,
                &name,
                data.status
                    .as_ref()
                    .and_then(|status| status.channel.as_ref())
                    .cloned(),
                DashJobState::Deleting,
            )
            .await;
        }

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            DashJobState::Pending => match validator.create(data.as_ref().clone()).await {
                Ok(channel) => {
                    Self::update_spec_or_requeue(
                        &namespace,
                        &manager.kube,
                        &name,
                        Some(channel),
                        DashJobState::Running,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to spawn dash jobs ({namespace}/{name}): {e}");
                    Self::update_spec_or_requeue(
                        &namespace,
                        &manager.kube,
                        &name,
                        None,
                        DashJobState::Error,
                    )
                    .await
                    .map(|_| Action::await_change())
                }
            },
            DashJobState::Running => match validator.is_running(data.as_ref().clone()).await {
                Ok(true) => Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                )),
                Ok(false) => match validator.delete(data.as_ref().clone()).await {
                    Ok(channel) => Self::update_spec_or_requeue(
                        &namespace,
                        &manager.kube,
                        &name,
                        Some(channel),
                        DashJobState::Completed,
                    )
                    .await
                    .map(|_| Action::await_change()),
                    Err(e) => {
                        warn!("failed to delete dash job ({namespace}/{name}): {e}");
                        Ok(Action::requeue(
                            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                        ))
                    }
                },
                Err(e) => {
                    warn!("failed to check dash job state ({namespace}/{name}): {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
            DashJobState::Error | DashJobState::Completed => {
                if data
                    .status
                    .as_ref()
                    .map(|status| now - status.last_updated >= completed_job_gc_timeout)
                    .unwrap_or(true)
                {
                    warn!(
                        "cleaning up {state} job: {namespace}/{name}",
                        state = data.status.as_ref().map(|status| status.state).unwrap(),
                    );
                    Self::delete_or_requeue(&namespace, &manager.kube, &name).await
                } else {
                    Ok(Action::requeue(completed_job_gc_timeout.to_std().unwrap()))
                }
            }
            DashJobState::Deleting => match validator.delete(data.as_ref().clone()).await {
                Ok(_) => Self::remove_finalizer_or_requeue(&namespace, &manager.kube, &name).await,
                Err(e) => {
                    warn!("failed to delete dash job ({namespace}/{name}): {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
        }
    }
}

impl Ctx {
    async fn update_spec_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        channel: Option<FunctionChannel>,
        state: DashJobState,
    ) -> Result<Action, Error> {
        match Self::update_spec(namespace, kube, name, channel, state).await {
            Ok(()) => {
                info!("dash job is {state}: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to update dash job state ({namespace}/{name} => {state}): {e}");
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
        channel: Option<FunctionChannel>,
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
                channel,
                state,
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }

    async fn remove_finalizer_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
    ) -> Result<Action, Error> {
        let api = Api::<<Self as ::ark_core_k8s::manager::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        match <Self as ::ark_core_k8s::manager::Ctx>::remove_finalizer(api, name).await {
            Ok(()) => {
                info!("dash job has finalized: {namespace}/{name}");
                Ok(Action::await_change())
            }
            Err(e) => {
                warn!("failed to finalize dash job state ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
    }

    async fn delete_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
    ) -> Result<Action, Error> {
        let api = Api::<<Self as ::ark_core_k8s::manager::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let dp = DeleteParams::default();

        match api.delete(name, &dp).await {
            Ok(_) => {
                info!("requested dash job deletion: {namespace}/{name}");
                Ok(Action::await_change())
            }
            Err(e) => {
                warn!("failed to remove dash job ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
    }
}
