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
            ModelStorageState::Pending => {
                let validator = ModelStorageValidator {
                    kubernetes_storage: KubernetesStorageClient {
                        namespace: &namespace,
                        kube: &manager.kube,
                    },
                };
                match validator.validate_model_storage(&name, &data.spec).await {
                    Ok(()) => {
                        Self::update_state_or_requeue(
                            &namespace,
                            &manager.kube,
                            &name,
                            data.spec.kind.clone(),
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
        }
    }
}

impl Ctx {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn update_state_or_requeue(
        namespace: &str,
        kube: &Client,
        name: &str,
        kind: ModelStorageKindSpec,
    ) -> Result<Action, Error> {
        match Self::update_state(namespace, kube, name, kind).await {
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
        kind: ModelStorageKindSpec,
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
                state: ModelStorageState::Ready,
                kind: Some(kind),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::ark_core_k8s::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
