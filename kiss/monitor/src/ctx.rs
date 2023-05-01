use std::sync::Arc;

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use chrono::Utc;
use k8s_openapi::api::batch::v1::Job;
use kiss_api::{
    ansible::AnsibleClient,
    r#box::{BoxCrd, BoxState},
};
use kube::{
    api::{Patch, PatchParams},
    runtime::controller::Action,
    Api, CustomResourceExt, Error, ResourceExt,
};
use log::{info, warn};
use serde_json::json;

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = Job;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::kiss_api::consts::NAMESPACE;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();

        // skip reconciling if not managed
        let box_name: String = match Self::get_box_name(&data) {
            Some(e) => e,
            None => {
                info!("{} is not a target; skipping", &name);
                return Ok(Action::await_change());
            }
        };

        let status = data.status.as_ref();
        let completed_state = data
            .labels()
            .get(AnsibleClient::LABEL_COMPLETED_STATE)
            .and_then(|state| state.parse().ok());

        let has_completed = status.and_then(|e| e.succeeded).unwrap_or_default() > 0;
        let has_failed = status.and_then(|e| e.failed).unwrap_or_default() > 0;

        // when the ansible job is succeeded
        if has_completed {
            info!("Job has completed: {name} ({box_name})");

            // update the state
            if let Some(completed_state) = completed_state {
                info!("Updating box state: {name} ({box_name} => {completed_state})");
                Self::update_box_state(manager, data, completed_state).await
            }
            // keep the state, scheduled by the controller
            else {
                info!("Skipping updating box state: {name} ({box_name})");
                Ok(Action::requeue(
                    <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
                ))
            }
        }
        // when the ansible job is failed
        else if has_failed {
            let failed_state = BoxState::Failed;
            warn!("Job has failed: {name} ({box_name})");
            warn!("Updating box state: {name} ({box_name} => {failed_state})");

            Self::update_box_state(manager, data, failed_state).await
        }
        // when the ansible job is not finished yet
        else {
            Ok(Action::requeue(
                <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
            ))
        }
    }
}

impl Ctx {
    async fn update_box_state(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
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
            <Self as ::ark_core_k8s::manager::Ctx>::FALLBACK,
        ))
    }

    fn get_box_name(data: &<Self as ::ark_core_k8s::manager::Ctx>::Data) -> Option<String> {
        Self::get_label(data, AnsibleClient::LABEL_BOX_NAME)
    }

    fn get_label<T>(data: &<Self as ::ark_core_k8s::manager::Ctx>::Data, label: &str) -> Option<T>
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
