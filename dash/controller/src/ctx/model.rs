use std::{sync::Arc, time::Duration};

use dash_api::model::{ModelCrd, ModelFieldsSpec, ModelState, ModelStatus};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::{info, warn},
};
use kiss_api::{
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, Client, CustomResourceExt, Error, ResourceExt,
    },
    manager::Manager,
    serde_json::json,
};

use crate::validator::model::ModelValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = ModelCrd;

    const NAME: &'static str = "dash-controller";
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
            .unwrap_or(&ModelState::Pending)
        {
            ModelState::Pending => {
                let validator = ModelValidator {
                    kube: &manager.kube,
                };
                match validator.validate_model(&data.spec).await {
                    Ok(fields) => match Self::update_fields(&manager.kube, &name, fields).await {
                        Ok(()) => {
                            info!("model is ready: {name}");
                            Ok(Action::await_change())
                        }
                        Err(e) => {
                            warn!("failed to update model state {name:?}: {e}");
                            Ok(Action::requeue(
                                <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                            ))
                        }
                    },
                    Err(e) => {
                        warn!("failed to validate model: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            ModelState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
        }
    }
}

impl Ctx {
    async fn update_fields(kube: &Client, name: &str, fields: ModelFieldsSpec) -> Result<()> {
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(kube.clone());
        let crd = <Self as ::kiss_api::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": ModelStatus {
                state: Some(ModelState::Ready),
                fields: Some(fields),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::kiss_api::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
