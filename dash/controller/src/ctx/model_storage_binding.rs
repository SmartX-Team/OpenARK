use std::{sync::Arc, time::Duration};

use dash_actor::storage::KubernetesStorageClient;
use dash_api::{
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, Client, CustomResourceExt, Error, ResourceExt,
    },
    model::ModelSpec,
    model_storage_binding::{
        ModelStorageBindingCrd, ModelStorageBindingState, ModelStorageBindingStatus,
    },
    serde_json::json,
    storage::ModelStorageSpec,
};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::{info, warn},
};
use kiss_api::manager::Manager;

use crate::validator::{
    model::ModelValidator, model_storage_binding::ModelStorageBindingValidator,
    storage::ModelStorageValidator,
};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = ModelStorageBindingCrd;

    const NAME: &'static str = "dash-controller";
    const NAMESPACE: &'static str = ::dash_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();

        match data
            .status
            .as_ref()
            .and_then(|status| status.state.as_ref())
            .unwrap_or(&ModelStorageBindingState::Pending)
        {
            ModelStorageBindingState::Pending => {
                let kubernetes_storage = KubernetesStorageClient {
                    kube: &manager.kube,
                };
                let validator = ModelStorageBindingValidator {
                    model: ModelValidator { kubernetes_storage },
                    model_storage: ModelStorageValidator { kubernetes_storage },
                };
                match validator.validate_model_storage_binding(&data.spec).await {
                    Ok((model, storage)) => {
                        match Self::update_state(&manager.kube, &name, model, storage).await {
                            Ok(()) => {
                                info!("model storage binding is ready: {name}");
                                Ok(Action::await_change())
                            }
                            Err(e) => {
                                warn!("failed to update model storage binding state {name:?}: {e}");
                                Ok(Action::requeue(
                                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                                ))
                            }
                        }
                    }
                    Err(e) => {
                        warn!("failed to validate model storage binding: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelStorageBindingState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
        }
    }
}

impl Ctx {
    async fn update_state(
        kube: &Client,
        name: &str,
        model: ModelSpec,
        storage: ModelStorageSpec,
    ) -> Result<()> {
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(kube.clone());
        let crd = <Self as ::kiss_api::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStorageBindingStatus {
                state: Some(ModelStorageBindingState::Ready),
                model: Some(model),
                storage: Some(storage),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::kiss_api::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
