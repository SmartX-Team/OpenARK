use std::{sync::Arc, time::Duration};

use dash_actor::storage::KubernetesStorageClient;
use dash_api::{
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, Client, CustomResourceExt, Error, ResourceExt,
    },
    serde_json::json,
    storage::{ModelStorageCrd, ModelStorageKindSpec, ModelStorageState, ModelStorageStatus},
};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::{info, warn},
};
use kiss_api::manager::Manager;

use crate::validator::storage::ModelStorageValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = ModelStorageCrd;

    const NAME: &'static str = crate::consts::NAME;
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
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ModelStorageState::Pending => {
                let validator = ModelStorageValidator {
                    kubernetes_storage: KubernetesStorageClient {
                        kube: &manager.kube,
                    },
                };
                match validator.validate_model_storage(&data.spec).await {
                    Ok(()) => {
                        match Self::update_state(&manager.kube, &name, data.spec.kind.clone()).await
                        {
                            Ok(()) => {
                                info!("model storage is ready: {name}");
                                Ok(Action::await_change())
                            }
                            Err(e) => {
                                warn!("failed to update model storage state {name:?}: {e}");
                                Ok(Action::requeue(
                                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                                ))
                            }
                        }
                    }
                    Err(e) => {
                        warn!("failed to validate model storage: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
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
    async fn update_state(kube: &Client, name: &str, kind: ModelStorageKindSpec) -> Result<()> {
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(kube.clone());
        let crd = <Self as ::kiss_api::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStorageStatus {
                state: ModelStorageState::Ready,
                kind: Some(kind),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::kiss_api::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
