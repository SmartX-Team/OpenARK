use std::sync::Arc;

use ipis::{async_trait::async_trait, core::anyhow::Result, log::warn};
use kiss_api::{
    ansible::AnsibleClient,
    k8s_openapi::api::batch::v1::Job,
    kube::{runtime::controller::Action, Api, Error, ResourceExt},
    manager::Manager,
    r#box::{BoxCrd, BoxState, BoxStatus},
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
        let name = data.name();
        let status = data.status.as_ref();
        let api = Api::<<Self as ::kiss_api::manager::Ctx>::Data>::all(manager.kube.clone());

        // when the ansible job is succeeded
        if status.and_then(|e| e.succeeded) == Some(1) {
            let completed_state = match data
                .annotations()
                .get(AnsibleClient::ANNOTATION_COMPLETED_STATE)
            {
                Some(state) => state,
                None => {
                    warn!("cannot find the job's expected completed state: {name}");
                    return Ok(Action::requeue(
                        <Self as ::kiss_api::manager::Ctx>::FALLBACK,
                    ));
                }
            };

            dbg!(&completed_state);
            dbg!(&data);
            todo!()
        }
        // when the ansible job is failed
        else if status.and_then(|e| e.failed) == Some(1) {
            dbg!(&data);
            todo!()
        }
        // when the ansible job is not finished yet
        else {
            Ok(Action::requeue(
                <Self as ::kiss_api::manager::Ctx>::FALLBACK,
            ))
        }
    }

    fn error_policy<E>(_manager: Arc<Manager<Self>>, _error: E) -> Action
    where
        Self: Sized,
        E: ::std::fmt::Debug,
    {
        Action::await_change()
    }
}
