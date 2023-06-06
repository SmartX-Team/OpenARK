use std::sync::Arc;

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use k8s_openapi::api::core::v1::Node;
use kube::{runtime::controller::Action, Error, ResourceExt};
use log::{info, warn};
use vine_api::user::UserCrd;
use vine_session::SessionManager;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = Node;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::vine_api::consts::NAMESPACE;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let namespace = match data.labels().get(::ark_api::consts::LABEL_BIND_BY_USER) {
            Some(user_name) => UserCrd::user_namespace_with(user_name),
            None => {
                info!("skipping unbinding node ({name}): user not found");
                return Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ));
            }
        };

        let session_manager = match SessionManager::try_new(namespace, manager.kube.clone()).await {
            Ok(session_manager) => session_manager,
            Err(e) => {
                warn!("failed to creata a SessionManager: {e}");
                return Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ));
            }
        };

        match session_manager.try_delete(&data).await {
            Ok(Some(user_name)) => {
                info!("unbinded node: {name:?} => {user_name:?}");
            }
            Ok(None) => {}
            Err(e) => {
                warn!("failed to unbind node: {name:?}: {e}");
            }
        }

        // If no events were received, check back after a few minutes
        Ok(Action::await_change())
    }
}
