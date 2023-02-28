use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::anyhow::Result,
    log::{info, warn},
};
use kiss_api::{
    k8s_openapi::api::core::v1::Node,
    kube::{runtime::controller::Action, Error, ResourceExt},
    manager::Manager,
};
use vine_session::SessionManager;

#[derive(Default)]
pub struct Ctx {
    session_manager: SessionManager,
}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = Node;

    const NAME: &'static str = "vine-controller";

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();

        // if the node is not ready, then logout all sessions using it
        if !data
            .status
            .as_ref()
            .and_then(|status| status.conditions.as_ref())
            .and_then(|conditions| {
                conditions
                    .iter()
                    .find(|condition| condition.type_ == "Ready")
            })
            .map(|condition| condition.status == "True")
            .unwrap_or(false)
        {
            match manager
                .ctx
                .read()
                .await
                .session_manager
                .unbind(&manager.kube, &data)
                .await
            {
                Ok(Some(user_name)) => {
                    info!("unbinded node: {name:?} => {user_name:?}");
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("failed to unbind node: {name:?}: {e}");
                }
            }
        }

        // If no events were received, check back after a few minutes
        Ok(Action::await_change())
    }
}
