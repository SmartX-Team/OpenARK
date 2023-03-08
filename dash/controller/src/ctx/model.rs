use std::sync::Arc;

use ipis::{async_trait::async_trait, core::anyhow::Result};
use kiss_api::{
    k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
    kube::{runtime::controller::Action, CustomResourceExt, Error},
    manager::Manager,
};
use dash_api::model::ModelCrd;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = ModelCrd;

    const NAME: &'static str = "dash-controller";

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        vec![
            ::dash_api::function::FunctionCrd::crd(),
        ]
    }

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        // If no events were received, check back after a few minutes
        Ok(Action::requeue(
            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
        ))
    }
}
