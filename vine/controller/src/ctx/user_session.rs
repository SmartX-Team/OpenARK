use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::anyhow::Result,
    log::{info, warn},
};
use kiss_api::manager::Manager;
use vine_api::{
    k8s_openapi::api::core::v1::Node,
    kube::{runtime::controller::Action, Error, ResourceExt},
};
use vine_session::SessionManager;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = Node;

    const NAME: &'static str = "vine-controller";
    const NAMESPACE: &'static str = ::vine_api::consts::NAMESPACE;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let session_manager = match SessionManager::try_new(manager.kube.clone()).await {
            Ok(session_manager) => session_manager,
            Err(e) => {
                warn!("failed to creata a SessionManager: {e}");
                return Ok(Action::requeue(
                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                ));
            }
        };

        let name = data.name_any();

        match session_manager.try_unbind(&data).await {
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
