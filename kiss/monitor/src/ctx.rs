use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::warn,
};
use kiss_api::{
    ansible::AnsibleClient,
    k8s_openapi::api::batch::v1::Job,
    kube::{
        api::{DeleteParams, Patch, PatchParams},
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
    type Data = Job;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let status = data.status.as_ref();
        let completed_state = data
            .labels()
            .get(AnsibleClient::LABEL_COMPLETED_STATE)
            .and_then(|state| state.parse().ok())
            .unwrap_or(BoxState::Failed);

        let has_completed = status.and_then(|e| e.succeeded).unwrap_or_default() > 0;
        let has_failed = status.and_then(|e| e.failed).unwrap_or_default() > 0;

        // when the ansible job is succeeded
        if has_completed {
            let fallback_state = completed_state.fail();
            match fallback_state {
                BoxState::Failed => Self::update_box_state(manager, data, fallback_state).await,
                // do nothing when the job has no fallback state
                _ => Ok(Action::await_change()),
            }
            // Self::update_box_state(manager, data, completed_state).await
        }
        // when the ansible job is failed
        else if has_failed {
            let fallback_state = completed_state.fail();
            match fallback_state {
                BoxState::Failed => Self::update_box_state(manager, data, fallback_state).await,
                // do nothing when the job has no fallback state
                _ => Ok(Action::await_change()),
            }
        }
        // when the ansible job is not finished yet
        else {
            Ok(Action::await_change())
        }
    }

    fn error_policy<E>(_manager: Arc<Manager<Self>>, _error: E) -> Action
    where
        Self: Sized,
        E: ::std::fmt::Debug,
    {
        Action::requeue(<Self as ::kiss_api::manager::Ctx>::FALLBACK)
    }
}

impl Ctx {
    async fn update_box_state(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
        state: BoxState,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        // delete the job
        {
            let ns = "kiss";
            let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::namespaced(
                manager.kube.clone(),
                ns,
            );

            let dp = DeleteParams::background();
            api.delete(&data.name(), &dp).await?;
        }

        // update the box
        {
            let box_name: String = match Self::get_label(&data, AnsibleClient::LABEL_BOX_NAME).await
            {
                Some(e) => e,
                None => return Ok(Action::await_change()),
            };

            let api = Api::<BoxCrd>::all(manager.kube.clone());
            let crd = BoxCrd::api_resource();
            let patch = Patch::Apply(json!({
                "apiVersion": crd.api_version,
                "kind": crd.kind,
                "status": BoxStatus {
                    state,
                    last_updated: Utc::now(),
                },
            }));
            let pp = PatchParams::apply("kiss-monitor").force();
            api.patch_status(&box_name, &pp, &patch).await?;
        }

        Ok(Action::await_change())
    }

    async fn get_label<T>(
        data: &Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
        label: &str,
    ) -> Option<T>
    where
        T: ::core::str::FromStr + Send,
        <T as ::core::str::FromStr>::Err: ::core::fmt::Display + Send,
    {
        match data.labels().get(label) {
            Some(value) => match value.parse() {
                Ok(value) => Some(value),
                Err(e) => {
                    warn!("failed to parse the {label} label of {}: {e}", data.name(),);
                    None
                }
            },
            None => {
                warn!("failed to get the {label} label: {}", data.name());
                None
            }
        }
    }
}
