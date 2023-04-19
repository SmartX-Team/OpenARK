use std::sync::Arc;

use ark_api::package::{ArkPackageCrd, ArkPackageState};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::{info, warn},
};
use kiss_api::{
    k8s_openapi::api::batch::v1::Job,
    kube::{
        api::{Patch, PatchParams},
        runtime::controller::Action,
        Api, Client, CustomResourceExt, Error, ResourceExt,
    },
    manager::Manager,
    serde_json::json,
};

#[derive(Default)]
pub struct Ctx {}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = Job;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::ark_api::consts::NAMESPACE;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let kube = &manager.kube;
        let namespace = data.namespace().unwrap_or_else(|| "default".into());
        let name = data.name_any();

        let parse_label = |key: &str| data.labels().get(key);
        let build_timestamp =
            match parse_label(::ark_actor_kubernetes::consts::LABEL_BUILD_TIMESTAMP) {
                Some(build_timestamp) => build_timestamp,
                None => return Ok(Action::await_change()),
            };
        let package_name = match parse_label(::ark_actor_kubernetes::consts::LABEL_PACKAGE_NAME) {
            Some(build_timestamp) => build_timestamp,
            None => return Ok(Action::await_change()),
        };

        let status = data.status.as_ref();
        let has_completed = status.and_then(|e| e.succeeded).unwrap_or_default() > 0;
        let has_failed = status.and_then(|e| e.failed).unwrap_or_default() > 0;

        // when the ansible job is succeeded
        if has_completed {
            info!("Job has completed: {name} ({package_name})");

            // update the state
            let ctx = UpdateStateCtx {
                kube,
                namespace,
                name,
                build_timestamp,
                package_name,
                state: ArkPackageState::Ready,
            };
            ctx.apply().await
        }
        // when the ansible job is failed
        else if has_failed {
            let ctx = UpdateStateCtx {
                kube,
                namespace,
                name,
                build_timestamp,
                package_name,
                state: ArkPackageState::Failed,
            };
            ctx.apply().await
        }
        // when the ansible job is not finished yet
        else {
            Ok(Action::requeue(
                <Self as ::kiss_api::manager::Ctx>::FALLBACK,
            ))
        }
    }
}

struct UpdateStateCtx<'a> {
    kube: &'a Client,
    namespace: String,
    name: String,
    build_timestamp: &'a String,
    package_name: &'a String,
    state: ArkPackageState,
}

impl<'a> UpdateStateCtx<'a> {
    async fn apply(&self) -> Result<Action, Error> {
        match self.try_apply().await {
            Ok(()) => Ok(Action::await_change()),
            Err(e) => {
                let package_name = self.package_name;
                warn!("failed to update package state: {package_name}: {e}");

                Err(Error::Service(e.into()))
            }
        }
    }

    async fn try_apply(&self) -> Result<()> {
        let Self {
            kube,
            namespace,
            name,
            build_timestamp,
            package_name,
            state,
        } = self;
        info!("Updating package state: {name} ({package_name} => {state})");

        let api = Api::<ArkPackageCrd>::namespaced((*kube).clone(), namespace);
        match api.get_opt(package_name).await {
            Ok(Some(package))
                if package
                    .labels()
                    .get(::ark_actor_kubernetes::consts::LABEL_BUILD_TIMESTAMP)
                    == Some(*build_timestamp) =>
            {
                if package
                    .status
                    .as_ref()
                    .map(|status| status.state == ArkPackageState::Building)
                    .unwrap_or_default()
                {
                    let crd = ArkPackageCrd::api_resource();
                    let patch = Patch::Merge(json!({
                        "apiVersion": crd.api_version,
                        "kind": crd.kind,
                        "status": {
                            "state": state,
                            "last_updated": Utc::now(),
                        },
                    }));
                    let pp = PatchParams::apply(<Ctx as ::kiss_api::manager::Ctx>::NAME);
                    api.patch_status(package_name, &pp, &patch).await?;

                    info!("updated package state; removing job: {name}");
                    super::try_delete::<Job>(kube, namespace, name).await
                } else {
                    info!(
                        "package is not in building state; skipping updating state: {package_name}"
                    );
                    Ok(())
                }
            }
            Ok(_) => {
                info!("build timestamp mismatch; skipping updating state: {package_name}");
                Ok(())
            }
            Err(e) => {
                info!("failed to find package; skipping updating state: {package_name}: {e}");
                Ok(())
            }
        }
    }
}
