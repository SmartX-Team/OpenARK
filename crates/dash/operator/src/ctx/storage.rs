use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use dash_api::storage::{
    ModelStorageCrd, ModelStorageKindSpec, ModelStorageState, ModelStorageStatus,
};
use dash_provider::storage::KubernetesStorageClient;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use serde_json::json;
use tracing::{info, instrument, warn, Level};

use crate::validator::storage::ModelStorageValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = ModelStorageCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::dash_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds
    const FINALIZER_NAME: &'static str =
        <Self as ::ark_core_k8s::manager::Ctx>::Data::FINALIZER_NAME;

    #[instrument(level = Level::INFO, skip_all, fields(name = %data.name_any(), namespace = data.namespace()), err(Display))]
    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let namespace = data.namespace().unwrap();

        if data.metadata.deletion_timestamp.is_some()
            && data
                .status
                .as_ref()
                .map(|status| status.state != ModelStorageState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            return Self::update_state_or_requeue(
                &namespace,
                &manager.kube,
                &name,
                status.and_then(|status| status.kind.clone()),
                ModelStorageState::Deleting,
                None,
            )
            .await;
        } else if !data
            .finalizers()
            .iter()
            .any(|finalizer| finalizer == <Self as ::ark_core_k8s::manager::Ctx>::FINALIZER_NAME)
        {
            return <Self as ::ark_core_k8s::manager::Ctx>::add_finalizer_or_requeue_namespaced(
                manager.kube.clone(),
                &namespace,
                &name,
            )
            .await;
        }

        let validator = ModelStorageValidator {
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
            ModelStorageState::Pending => {
                match validator.validate_model_storage(&name, &data.spec).await {
                    Ok(total_quota) => {
                        Self::update_state_or_requeue(
                            &namespace,
                            &manager.kube,
                            &name,
                            Some(data.spec.kind.clone()),
                            ModelStorageState::Ready,
                            total_quota,
                        )
                        .await
                    }
                    Err(e) => {
                        warn!("failed to validate model storage: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
            ModelStorageState::Deleting => match validator.delete(&data).await {
                Ok(()) => {
                    <Self as ::ark_core_k8s::manager::Ctx>::remove_finalizer_or_requeue_namespaced(
                        manager.kube.clone(),
                        &namespace,
                        &name,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to delete model storage ({namespace}/{name}): {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
        }
    }
}

impl Ctx {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn update_state_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        kind: Option<ModelStorageKindSpec>,
        state: ModelStorageState,
        total_quota: Option<u128>,
    ) -> Result<Action, Error> {
        match Self::update_state(namespace, kube, name, kind, state, total_quota).await {
            Ok(()) => {
                info!("model storage is ready: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to update model storage state ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
    }

    #[instrument(level = Level::INFO, skip(kube, kind), err(Display))]
    async fn update_state(
        namespace: &str,
        kube: &Client,
        name: &str,
        kind: Option<ModelStorageKindSpec>,
        state: ModelStorageState,
        total_quota: Option<u128>,
    ) -> Result<()> {
        let api = Api::<<Self as ::ark_core_k8s::manager::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::ark_core_k8s::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStorageStatus {
                state,
                kind,
                last_updated: Utc::now(),
                total_quota,
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
