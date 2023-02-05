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
    r#box::{BoxCrd, BoxGroupRole, BoxState, BoxStatus},
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
        let crd = BoxCrd::api_resource();
        let name = data.name_any();
        let status = data.status.as_ref();
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(manager.kube.clone());

        // get the current time
        let now = Utc::now();

        // load the box's state
        let old_state = status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or(BoxState::New);
        let mut new_state = old_state.next();
        let mut new_group = None;

        // wait until status updating timeout is end
        if let Some(timeout) = old_state.timeout_update() {
            if let Some(last_updated) = status
                .map(|status| &status.last_updated)
                .or_else(|| data.metadata.creation_timestamp.as_ref().map(|e| &e.0))
            {
                if now > *last_updated + timeout {
                    // wait new boxes with no access methods for begin provisioned
                    if matches!(old_state, BoxState::New)
                        && !status
                            .as_ref()
                            .map(|status| status.access.primary.is_some())
                            .unwrap_or_default()
                    {
                        // update the status
                        new_state = BoxState::Disconnected;
                    }
                } else {
                    return Ok(Action::requeue(timeout.to_std().unwrap()));
                }
            } else {
                return Ok(Action::requeue(timeout.to_std().unwrap()));
            }
        }

        // capture the timeout
        if let Some(last_updated) = status.map(|status| status.last_updated) {
            if let Some(time_threshold) = old_state.timeout() {
                if now > last_updated + time_threshold {
                    // update the status
                    new_state = BoxState::Failed;
                }
            }
        }

        // capture the group info is changed
        if matches!(old_state, BoxState::Running)
            && !status
                .as_ref()
                .and_then(|status| status.bind_group.as_ref())
                .map(|bind_group| &data.spec.group == bind_group)
                .unwrap_or_default()
        {
            new_state = BoxState::Disconnected;
        }

        if !matches!(old_state, BoxState::Joining) && matches!(new_state, BoxState::Joining) {
            // skip joining to default cluster as worker nodes when disabled
            if !manager.ansible.kiss.group_enable_default_cluster
                && data.spec.group.is_default()
                && !matches!(data.spec.group.role, BoxGroupRole::ControlPlane)
            {
                info!("Skipped joining (default cluster is disabled) {name:?}");
                return Ok(Action::requeue(
                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                ));
            }

            // skip joining if already joined
            if status
                .as_ref()
                .and_then(|status| status.bind_group.as_ref())
                .map(|bind_group| bind_group == &data.spec.group)
                .unwrap_or_default()
            {
                let patch = Patch::Apply(json!({
                    "apiVersion": crd.api_version,
                    "kind": crd.kind,
                    "status": BoxStatus {
                        state: BoxState::Running,
                        access: status.as_ref().map(|status| status.access.clone()).unwrap_or_default(),
                        bind_group: status.as_ref().and_then(|status| status.bind_group.clone()),
                        last_updated: Utc::now(),
                    },
                }));
                let pp = PatchParams::apply("kiss-controller").force();
                api.patch_status(&name, &pp, &patch).await?;

                info!("Skipped joining (already joined) {name:?}");
                return Ok(Action::requeue(
                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                ));
            }

            // bind to new group
            new_group = Some(&data.spec.group);
        }

        // spawn an Ansible job
        if old_state != new_state || new_state.cron().is_some() {
            if let Some(task) = new_state.as_task() {
                let is_spawned = manager
                    .ansible
                    .spawn(
                        &manager.kube,
                        AnsibleJob {
                            cron: new_state.cron(),
                            is_atomic: new_state.is_atomic(),
                            task,
                            r#box: &data,
                            new_group,
                            new_state,
                        },
                    )
                    .await?;

                // If there is a problem spawning a job, check back after a few minutes
                if !is_spawned {
                    info!("Cannot spawn an Ansible job; waiting: {}", &name);
                    return Ok(Action::requeue(
                        #[allow(clippy::identity_op)]
                        Duration::from_secs(1 * 60),
                    ));
                }
            }

            // wait for being changed
            if old_state == new_state {
                info!("Waiting for being changed: {name:?}");
                return Ok(Action::await_change());
            }

            let patch = Patch::Apply(json!({
                "apiVersion": crd.api_version,
                "kind": crd.kind,
                "status": BoxStatus {
                    state: new_state,
                    access: status.as_ref().map(|status| status.access.clone()).unwrap_or_default(),
                    bind_group: status.as_ref().and_then(|status| status.bind_group.clone()),
                    last_updated: Utc::now(),
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
