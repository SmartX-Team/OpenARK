use std::sync::Arc;

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{runtime::controller::Action, CustomResourceExt, Error, ResourceExt};
use tracing::{instrument, Level};
use vine_api::user_auth::UserAuthCrd;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = UserAuthCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::vine_api::consts::NAMESPACE;

    fn get_subcrds() -> Vec<CustomResourceDefinition> {
        vec![
            ::vine_api::user::UserCrd::crd(),
            ::vine_api::user_auth::UserAuthCrd::crd(),
            ::vine_api::user_auth_binding::UserAuthBindingCrd::crd(),
            ::vine_api::user_box_binding::UserBoxBindingCrd::crd(),
            ::vine_api::user_box_quota::UserBoxQuotaCrd::crd(),
            ::vine_api::user_box_quota_binding::UserBoxQuotaBindingCrd::crd(),
            ::vine_api::user_role::UserRoleCrd::crd(),
            ::vine_api::user_role_binding::UserRoleBindingCrd::crd(),
        ]
    }

    #[instrument(level = Level::INFO, skip_all, fields(name = data.name_any(), namespace = data.namespace()), err(Display))]
    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        // If no events were received, check back after a few minutes
        Ok(Action::requeue(
            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
        ))
    }
}
