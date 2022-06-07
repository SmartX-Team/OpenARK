use core::time::Duration;
use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::info,
};
use kiss_api::{
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, Error, ResourceExt,
    },
    manager::Manager,
    r#box::{BoxCrd, BoxState, BoxStatus},
    serde_json::json,
};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = BoxCrd;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let client = manager.client.clone();
        let name = data.name();
        let status = data.status.as_ref();
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(client);

        let old_state = status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or(BoxState::New);
        let new_state = match old_state {
            BoxState::New => BoxState::Commissioning,
            BoxState::Commissioning => BoxState::Commissioning,
            _ => todo!(),
        };

        let patch = Patch::Apply(json!({
            "apiVersion": "kiss.netai-cloud/v1alpha1",
            "kind": "Box",
            "status": BoxStatus {
                state: new_state,
                last_updated: if old_state == new_state {
                    status
                        .as_ref()
                        .map(|status| status.last_updated)
                        .unwrap_or_else(Utc::now)
                } else {
                    Utc::now()
                },
            },
        }));
        let pp = PatchParams::apply("kiss-controller").force();
        let _o = api.patch_status(&name, &pp, &patch).await?;

        info!("Reconciled Document {name:?}");

        // If no events were received, check back every 30 minutes
        Ok(Action::requeue(Duration::from_secs(30 * 60)))
    }

    fn error_policy<E>(_manager: Arc<Manager<Self>>, _error: E) -> Action
    where
        Self: Sized,
        E: ::std::fmt::Debug,
    {
        Action::await_change()
    }
}
