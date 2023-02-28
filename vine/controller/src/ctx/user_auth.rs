use std::sync::Arc;

use ipis::{async_trait::async_trait, core::anyhow::Result};
use kiss_api::{
    k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
    kube::{runtime::controller::Action, CustomResourceExt, Error},
    manager::Manager,
};
use vine_api::user_auth::UserAuthCrd;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = UserAuthCrd;

    const NAME: &'static str = "vine-controller";

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
