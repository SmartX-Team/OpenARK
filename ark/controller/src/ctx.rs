use std::{fmt, sync::Arc};

use ark_actor_api::repo::RepositoryManager;
use ark_actor_local::template::TemplateManager;
use ark_api::{
    package::{ArkPackageCrd, ArkPackageSpec, ArkPackageState, ArkPackageStatus},
    NamespaceAny,
};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, chrono::Utc},
    log::{info, warn},
    tokio::try_join,
};
use kiss_api::{
    k8s_openapi::{
        api::{
            batch::v1::{Job, JobSpec},
            core::v1::{
                ConfigMap, ConfigMapVolumeSource, Container, PodSpec, PodTemplateSpec, Volume,
            },
        },
        serde::{de::DeserializeOwned, Serialize},
        NamespaceResourceScope,
    },
    kube::{
        api::{DeleteParams, Patch, PatchParams, PostParams},
        core::ObjectMeta,
        runtime::controller::Action,
        Api, Client, CustomResourceExt, Error, Resource, ResourceExt,
    },
    manager::Manager,
    serde_json::json,
};

pub struct Ctx {
    repos: RepositoryManager,
    template: TemplateManager,
}

#[async_trait]
impl ::kiss_api::manager::TryDefault for Ctx {
    async fn try_default() -> Result<Self> {
        Ok(Self {
            repos: RepositoryManager::try_default().await?,
            template: TemplateManager::try_default().await?,
        })
    }
}

#[async_trait]
impl ::kiss_api::manager::Ctx for Ctx {
    type Data = ArkPackageCrd;

    const NAME: &'static str = crate::consts::NAME;
    const NAMESPACE: &'static str = ::ark_api::consts::NAMESPACE;

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();

        match data
            .status
            .as_ref()
            .and_then(|status| status.state.as_ref())
            .unwrap_or(&ArkPackageState::Pending)
        {
            ArkPackageState::Pending => build(&manager, &data).await,
            ArkPackageState::Building | ArkPackageState::Ready => {
                let status = data.status.as_ref().unwrap();

                if Some(&data.spec) != status.spec.as_ref() {
                    info!("package has been changed; rebuilding: {name}");
                    build(&manager, &data).await
                } else {
                    Ok(Action::await_change())
                }
            }
        }
    }
}

async fn build(
    manager: &Manager<Ctx>,
    data: &<Ctx as ::kiss_api::manager::Ctx>::Data,
) -> Result<Action, Error> {
    let name = data.name_any();
    let namespace = data.namespace_any();
    let job_name = format!("package-build-{name}");

    let metadata = ObjectMeta {
        labels: Some(
            [(::ark_actor_kubernetes::consts::LABEL_PACKAGE_NAME, &name)]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ),
        ..Default::default()
    };
    let object_metadata = ObjectMeta {
        name: Some(job_name.clone()),
        namespace: data.namespace(),
        ..metadata.clone()
    };

    let package = match manager.ctx.repos.get(&name).await {
        Ok(package) => package,
        Err(e) => {
            warn!("failed to find package: {namespace} -> {name}: {e}");
            return Ok(Action::requeue(<Ctx as ::kiss_api::manager::Ctx>::FALLBACK));
        }
    };
    let template = match manager.ctx.template.render_build(&package) {
        Ok(template) => template.text,
        Err(e) => {
            warn!("failed to render template: {namespace} -> {name}: {e}");
            return Ok(Action::requeue(<Ctx as ::kiss_api::manager::Ctx>::FALLBACK));
        }
    };
    let config_map = ConfigMap {
        metadata: object_metadata.clone(),
        data: Some(
            vec![("Containerfile".into(), template)]
                .into_iter()
                .collect(),
        ),
        ..Default::default()
    };

    let job = Job {
        metadata: object_metadata,
        spec: Some(JobSpec {
            template: PodTemplateSpec {
                metadata: Some(metadata),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "build".into(),
                        ..Default::default()
                    }],
                    volumes: Some(vec![Volume {
                        name: "template".into(),
                        config_map: Some(ConfigMapVolumeSource {
                            default_mode: Some(444),
                            name: Some(job_name),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        status: None,
    };

    match try_join!(
        force_create(&manager.kube, &config_map),
        force_create(&manager.kube, &job),
    ) {
        Ok(((), ())) => {
            info!("begin building: {namespace} -> {name}");
            match update_spec(&manager.kube, &name, data.spec.clone()).await {
                Ok(()) => Ok(Action::await_change()),
                Err(e) => {
                    info!("failed to update state: {namespace} -> {name}: {e}");
                    Ok(Action::await_change())
                }
            }
        }
        Err(e) => {
            warn!("failed to begin building: {namespace} -> {name}: {e}");
            Ok(Action::requeue(<Ctx as ::kiss_api::manager::Ctx>::FALLBACK))
        }
    }
}

async fn update_spec(kube: &Client, name: &str, spec: ArkPackageSpec) -> Result<()> {
    let api = Api::<<Ctx as ::kiss_api::manager::Ctx>::Data>::all(kube.clone());
    let crd = <Ctx as ::kiss_api::manager::Ctx>::Data::api_resource();

    let patch = Patch::Merge(json!({
        "apiVersion": crd.api_version,
        "kind": crd.kind,
        "status": ArkPackageStatus {
            state: Some(ArkPackageState::Building),
            spec: Some(spec),
            last_updated: Utc::now(),
        },
    }));
    let pp = PatchParams::apply(<Ctx as ::kiss_api::manager::Ctx>::NAME);
    api.patch_status(name, &pp, &patch).await?;
    Ok(())
}

async fn force_create<K>(kube: &Client, data: &K) -> Result<()>
where
    K: Clone
        + fmt::Debug
        + Serialize
        + DeserializeOwned
        + Resource<Scope = NamespaceResourceScope>
        + ResourceExt,
    <K as Resource>::DynamicType: Default,
{
    let name: String = data.name_any();
    let namespace = data.namespace_any();

    let api = Api::<K>::namespaced(kube.clone(), &namespace);
    if api.get_opt(&name).await?.is_some() {
        let dp = DeleteParams::default();
        api.delete(&name, &dp).await?;
    }

    let pp = PostParams {
        field_manager: Some(::ark_actor_kubernetes::consts::FIELD_MANAGER.into()),
        ..Default::default()
    };
    api.create(&pp, data).await.map(|_| ()).map_err(Into::into)
}
