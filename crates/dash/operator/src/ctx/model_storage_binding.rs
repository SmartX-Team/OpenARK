use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use dash_api::model_storage_binding::{
    ModelStorageBindingCrd, ModelStorageBindingState, ModelStorageBindingStatus,
};
use dash_provider::storage::KubernetesStorageClient;
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, Client, CustomResourceExt, Error, ResourceExt,
};
use serde_json::json;
use tracing::{info, instrument, warn, Level};

use crate::validator::{
    model::ModelValidator,
    model_storage_binding::{ModelStorageBindingValidator, UpdateContext},
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
                .map(|status| status.state != ModelStorageBindingState::Deleting)
                .unwrap_or(true)
        {
            let status = data.status.as_ref();
            return Self::update_state_or_requeue(
                &namespace,
                &manager.kube,
                &name,
                UpdateContext {
                    deletion_policy: status
                        .map(|status| status.deletion_policy)
                        .unwrap_or(data.spec.deletion_policy),
                    model: status.and_then(|status| status.model.clone()),
                    model_name: status.and_then(|status| status.model_name.clone()),
                    state: ModelStorageBindingState::Deleting,
                    storage_source: status
                        .and_then(|status| status.storage_source.as_ref())
                        .cloned(),
                    storage_source_binding_name: status
                        .and_then(|status| status.storage_source_binding_name.clone()),
                    storage_source_name: status
                        .and_then(|status| status.storage_source_name.clone()),
                    storage_sync_policy: status.and_then(|status| status.storage_sync_policy),
                    storage_target: status.and_then(|status| status.storage_target.clone()),
                    storage_target_name: status
                        .and_then(|status| status.storage_target_name.clone()),
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
            namespace: &namespace,
            name: &name,
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelStorageBindingState::Pending => {
                match validator.validate_model_storage_binding(&data.spec).await {
                    Ok(ctx) => {
                        Self::update_state_or_requeue(&namespace, &manager.kube, &name, ctx).await
                    }
                    Err(e) => {
                        warn!("failed to validate model storage binding: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageBindingState::Ready => match validator
                .update(&data.spec, data.status.as_ref().unwrap())
                .await
            {
                Ok(Some(ctx)) => {
                    Self::update_state_or_requeue(&namespace, &manager.kube, &name, ctx).await
                }
                Ok(None) => Ok(Action::await_change()),
                Err(e) => {
                    warn!("failed to update model storage binding: {name:?}: {e}");
                    Ok(Action::requeue(
                        <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                    ))
                }
            },
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
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn update_state_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        ctx: UpdateContext,
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

    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    async fn update_state(
        namespace: &str,
        kube: &Client,
        name: &str,
        UpdateContext {
            deletion_policy,
            model,
            model_name,
            state,
            storage_source,
            storage_source_binding_name,
            storage_source_name,
            storage_sync_policy,
            storage_target,
            storage_target_name,
        }: UpdateContext,
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
                model_name,
                storage_source,
                storage_source_binding_name,
                storage_source_name,
                storage_sync_policy,
                storage_target,
                storage_target_name,
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
