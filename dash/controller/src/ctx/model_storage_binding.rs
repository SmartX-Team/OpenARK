use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use dash_api::{
    model::ModelSpec,
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingDeletionPolicy, ModelStorageBindingState,
        ModelStorageBindingStatus, ModelStorageBindingStorageKind,
    },
    storage::ModelStorageSpec,
};
use dash_provider::storage::KubernetesStorageClient;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use serde_json::json;
use tracing::{info, warn};

use crate::validator::{
    model::ModelValidator, model_storage_binding::ModelStorageBindingValidator,
    storage::ModelStorageValidator,
};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = ModelStorageBindingCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::dash_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds
    const FINALIZER_NAME: &'static str =
        <Self as ::ark_core_k8s::manager::Ctx>::Data::FINALIZER_NAME;

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
                .map(|status| status.state != ModelStorageBindingState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            return Self::update_state_or_requeue(
                &namespace,
                &manager.kube,
                &name,
                UpdateCtx {
                    deletion_policy: status
                        .map(|status| status.deletion_policy)
                        .unwrap_or(data.spec.deletion_policy),
                    model: status.and_then(|status| status.model.as_ref()).cloned(),
                    storage: status.and_then(|status| status.storage.as_ref()).cloned(),
                    state: ModelStorageBindingState::Deleting,
                },
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

        let kubernetes_storage = KubernetesStorageClient {
            namespace: &namespace,
            kube: &manager.kube,
        };
        let validator = ModelStorageBindingValidator {
            model: ModelValidator { kubernetes_storage },
            model_storage: ModelStorageValidator { kubernetes_storage },
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelStorageBindingState::Pending => {
                match validator.validate_model_storage_binding(&data.spec).await {
                    Ok((model, storage)) => {
                        Self::update_state_or_requeue(
                            &namespace,
                            &manager.kube,
                            &name,
                            UpdateCtx {
                                deletion_policy: data.spec.deletion_policy,
                                model: Some(model),
                                storage: Some(storage),
                                state: ModelStorageBindingState::Ready,
                            },
                        )
                        .await
                    }
                    Err(e) => {
                        warn!("failed to validate model storage binding: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageBindingState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
            ModelStorageBindingState::Deleting => match validator.delete(&data.spec).await {
                Ok(()) => {
                    <Self as ::ark_core_k8s::manager::Ctx>::remove_finalizer_or_requeue_namespaced(
                        manager.kube.clone(),
                        &namespace,
                        &name,
                    )
                    .await
                }
                Err(e) => {
                    warn!("failed to delete model storage binding ({namespace}/{name}): {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
        }
    }
}

impl Ctx {
    async fn update_state_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        ctx: UpdateCtx,
    ) -> Result<Action, Error> {
        match Self::update_state(namespace, kube, name, ctx).await {
            Ok(()) => {
                info!("model storage binding is ready: {namespace}/{name}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
            Err(e) => {
                warn!("failed to validate model storage binding ({namespace}/{name}): {e}");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
    }

    async fn update_state(
        namespace: &str,
        kube: &Client,
        name: &str,
        UpdateCtx {
            deletion_policy,
            model,
            storage,
            state,
        }: UpdateCtx,
    ) -> Result<()> {
        let api = Api::<<Self as ::ark_core_k8s::manager::Ctx>::Data>::namespaced(
            kube.clone(),
            namespace,
        );
        let crd = <Self as ::ark_core_k8s::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStorageBindingStatus {
                state,
                deletion_policy,
                model,
                storage,
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}

struct UpdateCtx {
    deletion_policy: ModelStorageBindingDeletionPolicy,
    model: Option<ModelSpec>,
    storage: Option<ModelStorageBindingStorageKind<ModelStorageSpec>>,
    state: ModelStorageBindingState,
}
