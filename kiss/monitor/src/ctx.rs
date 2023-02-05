use std::sync::Arc;

use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::{info, warn},
};
use kiss_api::{
    ansible::AnsibleClient,
    k8s_openapi::api::batch::v1::Job,
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, CustomResourceExt, Error, ResourceExt,
    },
    manager::Manager,
    r#box::{BoxCrd, BoxState},
    serde_json::json,
};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = Job;

    const NAMESPACE: Option<&'static str> = Some(::kiss_api::consts::NAMESPACE);

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();

        // skip reconciling if not managed
        if Self::get_box_name(&data).is_none() {
            info!("{name} is not a target; skipping");
            return Ok(Action::await_change());
        }

        let status = data.status.as_ref();
        let has_completed = status.and_then(|e| e.succeeded).unwrap_or_default() > 0;
        let has_failed = status.and_then(|e| e.failed).unwrap_or_default() > 0;

        // when the ansible job is succeeded
        if has_completed {
            info!("Job has completed: {name}");

            Ok(Action::requeue(
                <Self as ::kiss_api::manager::Ctx>::FALLBACK,
            ))
        }
        // when the ansible job is failed
        else if has_failed {
            let state = BoxState::Failed;
            warn!("Job has failed: {name}");
            warn!("Updating box state: {name} => {state}");

            Self::update_box_state(manager, data, state).await
        }
        // when the ansible job is not finished yet
        else {
            Ok(Action::requeue(
                <Self as ::kiss_api::manager::Ctx>::FALLBACK,
            ))
        }
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
        // box name is already tested by reconciling
        let box_name = Self::get_box_name(&data).unwrap();

        // update the box
        {
            let api = Api::<BoxCrd>::all(manager.kube.clone());
            let crd = BoxCrd::api_resource();
            let patch = Patch::Apply(json!({
                "apiVersion": crd.api_version,
                "kind": crd.kind,
                "status": {
                    "state": state,
                    "lastUpdated": Utc::now(),
                },
            }));
            let pp = PatchParams::apply("kiss-monitor").force();
            api.patch_status(&box_name, &pp, &patch).await?;
        }

        info!("Updated box state: {} -> {}", &box_name, &state);
        Ok(Action::requeue(
            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
        ))
    }

    fn get_box_name(data: &<Self as ::kiss_api::manager::Ctx>::Data) -> Option<String> {
        Self::get_label(data, AnsibleClient::LABEL_BOX_NAME)
    }

    fn get_label<T>(data: &<Self as ::kiss_api::manager::Ctx>::Data, label: &str) -> Option<T>
    where
        T: ::core::str::FromStr + Send,
        <T as ::core::str::FromStr>::Err: ::core::fmt::Display + Send,
    {
        match data.labels().get(label) {
            Some(value) => match value.parse() {
                Ok(value) => Some(value),
                Err(e) => {
                    warn!(
                        "failed to parse the {label} label of {}: {e}",
                        data.name_any(),
                    );
                    None
                }
            },
            None => {
                info!("failed to get the {label} label: {}", data.name_any());
                None
            }
        }
    }
}
