use std::{sync::Arc, time::Duration};

use dash_api::{
    function::{FunctionCrd, FunctionSpec, FunctionState, FunctionStatus},
    model::ModelFieldKindNativeSpec,
};
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

use crate::validator::function::FunctionValidator;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = FunctionCrd;

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
            .unwrap_or(&FunctionState::Pending)
        {
            FunctionState::Pending => {
                let validator = FunctionValidator {
                    kube: &manager.kube,
                };
                match validator.validate_function(data.spec.clone()).await {
                    Ok(spec) => match Self::update_spec(&manager.kube, &name, spec).await {
                        Ok(()) => {
                            info!("function is ready: {name}");
                            Ok(Action::await_change())
                        }
                        Err(e) => {
                            warn!("failed to update function state {name:?}: {e}");
                            Ok(Action::requeue(
                                <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                            ))
                        }
                    },
                    Err(e) => {
                        warn!("failed to validate function: {name:?}: {e}");
                        Ok(Action::requeue(
                            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                        ))
                    }
                }
            }
            FunctionState::Ready => {
                // TODO: implement to finding changes
                Ok(Action::await_change())
            }
        }
    }
}

impl Ctx {
    async fn update_spec(
        kube: &Client,
        name: &str,
        spec: FunctionSpec<ModelFieldKindNativeSpec>,
    ) -> Result<()> {
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(kube.clone());
        let crd = <Self as ::kiss_api::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": FunctionStatus {
                state: Some(FunctionState::Ready),
                spec: Some(spec),
                last_updated: Utc::now(),
            },
        }));
        let pp = PatchParams::apply(<Self as ::kiss_api::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        Ok(())
    }
}
