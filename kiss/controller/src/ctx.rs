use std::{sync::Arc, time::Duration};

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
        let name = data.name_any();
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

        // capture the group info is changed
        if matches!(old_state, BoxState::Running)
            && !data
                .status
                .as_ref()
                .and_then(|status| status.bind_group.as_ref())
                .map(|bind_group| &data.spec.group == bind_group)
                .unwrap_or_default()
        {
            new_state = BoxState::Disconnected;
        }
        // capture the presence of access info
        else if !data
            .status
            .as_ref()
            .map(|status| status.access.is_some())
            .unwrap_or_default()
        {
            new_state = BoxState::Missing;
        }

        // spawn an Ansible job
        if old_state != new_state || new_state.cron().is_some() {
            if let Some(task) = new_state.as_task() {
                let is_spawned = manager
                    .ansible
                    .spawn(
                        &manager.cluster,
                        &manager.kube,
                        AnsibleJob {
                            cron: new_state.cron(),
                            task,
                            r#box: &*data,
                            new_state,
                            completed_state: new_state.complete(),
                        },
                    )
                    .await?;

                // If there is a problem spawning a job, check back after a few minutes
                if !is_spawned {
                    info!("Cannot spawn an Ansible job; waiting: {}", &name);
                    return Ok(Action::requeue(Duration::from_secs(1 * 60)));
                }
            }

            // wait for being changed
            if old_state == new_state {
                info!("Waiting for being changed: {name:?}");
                return Ok(Action::await_change());
            }

            let crd = BoxCrd::api_resource();
            let patch = Patch::Apply(json!({
                "apiVersion": crd.api_version,
                "kind": crd.kind,
                "status": BoxStatus {
                    state: new_state,
                    access: status.as_ref().and_then(|status| status.access.clone()),
                    bind_group: status.as_ref().and_then(|status| status.bind_group.clone()),
                    last_updated: status
                        .as_ref()
                        .map(|status| status.last_updated)
                        .unwrap_or_else(Utc::now),
                },
            }));
            let pp = PatchParams::apply("kiss-controller").force();
            api.patch_status(&name, &pp, &patch).await?;

            info!("Reconciled Document {name:?}");
        }

        // If no events were received, check back after a few minutes
        Ok(Action::requeue(
            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
        ))
    }
}
