use std::{sync::Arc, time::Duration};

use anyhow::Result;
use ark_core_k8s::manager::Manager;
use async_trait::async_trait;
use kube::{runtime::controller::Action, Error, ResourceExt};
use kubegraph_api::problem::NetworkProblemCrd;
use tracing::{instrument, Level};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::ark_core_k8s::manager::Ctx for Ctx {
    type Data = NetworkProblemCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::kubegraph_api::consts::NAMESPACE;
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    #[instrument(level = Level::INFO, skip_all, fields(name = %_data.name_any(), namespace = _data.namespace()), err(Display))]
    async fn reconcile(
        _manager: Arc<Manager<Self>>,
        _data: Arc<<Self as ::ark_core_k8s::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        Ok(Action::await_change())
    }
}
