use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::info,
};
use kiss_api::{
    ansible::AnsibleJob,
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, CustomResourceExt, Error, ResourceExt,
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
        let name = data.name();
        let status = data.status.as_ref();
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(manager.kube.clone());

        let old_state = status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or(BoxState::New);
        let mut new_state = old_state.next();

        // capture the timeout
        let now = Utc::now();
        if let Some(last_updated) = status.map(|status| status.last_updated) {
            if let Some(time_threshold) = old_state.timeout() {
                if now > last_updated + time_threshold {
                    // update the status
                    new_state = old_state.fail();
                }
            }
        }
        let completed_state = new_state.complete();

        // spawn an Ansible job
        if old_state != new_state {
            if let Some(task) = new_state.as_task() {
                manager
                    .ansible
                    .spawn(
                        &manager.kube,
                        AnsibleJob {
                            task,
                            access: &data.spec.access,
                            machine: &data.spec.machine,
                            completed_state,
                        },
                    )
                    .await?;
            }
        }

        let crd = BoxCrd::api_resource();
        let patch = Patch::Apply(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "status": BoxStatus {
                state: new_state,
                last_updated: if old_state == new_state {
                    status
                        .as_ref()
                        .map(|status| status.last_updated)
                        .unwrap_or_else(Utc::now)
                } else {
                    now
                },
            },
        }));
        let pp = PatchParams::apply("kiss-controller").force();
        api.patch_status(&name, &pp, &patch).await?;

        if old_state != new_state {
            info!("Reconciled Document {name:?}");
        }

        // If no events were received, check back after a few minutes
        Ok(Action::requeue(
            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
        ))
    }

    fn error_policy<E>(_manager: Arc<Manager<Self>>, _error: E) -> Action
    where
        Self: Sized,
        E: ::std::fmt::Debug,
    {
        Action::await_change()
    }
}
