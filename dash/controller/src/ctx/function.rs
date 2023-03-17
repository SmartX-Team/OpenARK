use std::{sync::Arc, time::Duration};

use dash_api::function::{FunctionCrd, FunctionSpec, FunctionState, FunctionStatus};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    futures::TryFutureExt,
    log::{info, warn},
};
use kiss_api::{
    k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
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
    type Data = FunctionCrd;

    const NAME: &'static str = "dash-controller";
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        vec![::dash_api::function::FunctionCrd::crd()]
    }

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
                let validator = ModelValidator {
                    kube: &manager.kube,
                };
                match validator
                    .validate_fields(&data.spec.input)
                    .and_then(|input| async {
                        let output = match data.spec.output.as_ref() {
                            Some(output) => validator.validate_fields(output).map_ok(Some).await?,
                            None => None,
                        };

                        Ok((input, output))
                    })
                    .await
                {
                    Ok((input, output)) => {
                        let spec = FunctionSpec {
                            input,
                            output,
                            actor: data.spec.actor.clone(),
                        };

                        match Self::update_spec(&manager.kube, &name, spec).await {
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
                        }
                    }
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
    async fn update_spec(kube: &Client, name: &str, spec: FunctionSpec) -> Result<()> {
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
