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
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, CustomResourceExt, Error, ResourceExt,
    },
    manager::Manager,
    r#box::{BoxCrd, BoxGroupSpec, BoxState, BoxStatus},
    serde_json::json,
};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = Job;

    const NAMESPACE: Option<&'static str> = Some("kiss");

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
            .and_then(|state| state.parse().ok());
        let group = None.or_else(|| {
            Some(BoxGroupSpec {
                cluster_name: data
                    .labels()
                    .get(AnsibleClient::LABEL_GROUP_CLUSTER_NAME)
                    .cloned()?,
                role: data
                    .labels()
                    .get(AnsibleClient::LABEL_GROUP_ROLE)
                    .and_then(|e| e.parse().ok())?,
            })
        });

        let has_completed = status.and_then(|e| e.succeeded).unwrap_or_default() > 0;
        let has_failed = status.and_then(|e| e.failed).unwrap_or_default() > 0;

        // when the ansible job is succeeded
        if has_completed {
            // update the state
            if let Some(completed_state) = completed_state {
                Self::update_box_state(manager, data, completed_state, group).await
            }
            // keep the state, scheduled by the controller
            else {
                Ok(Action::requeue(
                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                ))
            }
        }
        // when the ansible job is failed
        else if has_failed {
            let fallback_state = completed_state.unwrap_or(BoxState::Failed).fail();
            match fallback_state {
                BoxState::Failed => {
                    Self::update_box_state(manager, data, fallback_state, group).await
                }
                // do nothing when the job has no fallback state
                _ => Ok(Action::requeue(
                    <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                )),
            }
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
        group: Option<BoxGroupSpec>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        // update the box
        {
            let box_name: String = match Self::get_label(&data, AnsibleClient::LABEL_BOX_NAME).await
            {
                Some(e) => e,
                None => {
                    return Ok(Action::requeue(
                        <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                    ))
                }
            };

            let api = Api::<BoxCrd>::all(manager.kube.clone());
            let crd = BoxCrd::api_resource();
            let patch = Patch::Apply(json!({
                "apiVersion": crd.api_version,
                "kind": crd.kind,
                "status": BoxStatus {
                    state,
                    bind_group: group,
                    last_updated: Utc::now(),
                },
            }));
            let pp = PatchParams::apply("kiss-monitor").force();
            api.patch_status(&box_name, &pp, &patch).await?;
        }

        Ok(Action::requeue(
            <Self as ::kiss_api::manager::Ctx>::FALLBACK,
        ))
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
                    warn!(
                        "failed to parse the {label} label of {}: {e}",
                        data.name_any(),
                    );
                    None
                }
            },
            None => {
                warn!("failed to get the {label} label: {}", data.name_any());
                None
            }
        }
    }
}
