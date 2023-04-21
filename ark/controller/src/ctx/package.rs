use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};

use ark_actor_api::{args::ActorArgs, repo::RepositoryManager, runtime::ApplicationRuntime};
use ark_actor_kubernetes::PackageManager;
use ark_actor_local::template::TemplateManager;
use ark_api::{
    package::{ArkPackageCrd, ArkPackageSpec, ArkPackageState},
    NamespaceAny,
};
use ipis::{
    async_trait::async_trait,
    core::{
        anyhow::Result,
        chrono::{DateTime, NaiveDateTime, Utc},
    },
    env,
    log::{info, warn},
    tokio::{time, try_join},
};
use kiss_api::{
    k8s_openapi::{
        api::{
            batch::v1::{Job, JobSpec},
            core::v1::{
                Affinity, ConfigMap, ConfigMapVolumeSource, Container, EmptyDirVolumeSource,
                EnvVar, NodeAffinity, NodeSelectorRequirement, NodeSelectorTerm, Pod,
                PodSecurityContext, PodSpec, PodTemplateSpec, PreferredSchedulingTerm,
                ResourceRequirements, Volume, VolumeMount,
            },
        },
        apimachinery::pkg::api::resource::Quantity,
        serde::{de::DeserializeOwned, Serialize},
        NamespaceResourceScope,
    },
    kube::{
        api::{Patch, PatchParams, PostParams},
        core::ObjectMeta,
        runtime::controller::Action,
        Api, Client, CustomResourceExt, Error, Resource, ResourceExt,
    },
    manager::Manager,
    serde_json::json,
};

pub struct Ctx {
    app: ApplicationRuntime<()>,
    pull: bool,
    repos: RepositoryManager,
    template: TemplateManager,
}

#[async_trait]
impl ::kiss_api::manager::TryDefault for Ctx {
    async fn try_default() -> Result<Self> {
        Ok(Self {
            app: ApplicationRuntime::try_default()?,
            pull: env::infer(ActorArgs::ARK_PULL_KEY).unwrap_or(ActorArgs::ARK_PULL_VALUE),
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
    const FALLBACK: Duration = Duration::from_secs(30); // 30 seconds

    async fn reconcile(
        manager: Arc<Manager<Self>>,
        data: Arc<<Self as ::kiss_api::manager::Ctx>::Data>,
    ) -> Result<Action, Error>
    where
        Self: Sized,
    {
        let name = data.name_any();
        let args = BuildArgs {
            manager: &manager,
            data: &data,
        };

        let is_changed = || {
            data.status
                .as_ref()
                .and_then(|status| status.spec.as_ref())
                .map(|last| &data.spec != last)
                .unwrap_or(true)
        };

        let rebuild = || async {
            info!("package has been changed; rebuilding: {name}");
            begin_build(&args).await
        };

        let rebuild_if_changed = || async {
            if is_changed() {
                rebuild().await
            } else {
                Ok(Action::await_change())
            }
        };

        match data
            .status
            .as_ref()
            .map(|status| status.state)
            .unwrap_or_default()
        {
            ArkPackageState::Pending => begin_build(&args).await,
            ArkPackageState::Building => {
                if is_changed() {
                    rebuild().await
                } else {
                    match Self::TIMEOUT_BUILDING
                        .and_then(|timeout| ::ipis::core::chrono::Duration::from_std(timeout).ok())
                    {
                        Some(timeout) => match data
                            .labels()
                            .get(::ark_actor_kubernetes::consts::LABEL_BUILD_TIMESTAMP)
                            .and_then(|build_timestamp| build_timestamp.parse::<i64>().ok())
                            .and_then(|build_timestamp| {
                                NaiveDateTime::from_timestamp_micros(build_timestamp)
                            })
                            .map(|build_timestamp| DateTime::<Utc>::from_utc(build_timestamp, Utc))
                            .and_then(|build_timestamp| build_timestamp.checked_add_signed(timeout))
                        {
                            Some(build_timeout) => {
                                let now = Utc::now();
                                if now > build_timeout {
                                    let reason = "timeout";
                                    cancel_build(&manager, &data, reason).await
                                } else {
                                    match (now - build_timeout).to_std() {
                                        Ok(remaining) => Ok(Action::requeue(remaining)),
                                        Err(_) => Ok(Action::await_change()),
                                    }
                                }
                            }
                            None => match timeout.to_std() {
                                Ok(timeout) => Ok(Action::requeue(timeout)),
                                Err(_) => Ok(Action::await_change()),
                            },
                        },
                        None => Ok(Action::await_change()),
                    }
                }
            }
            ArkPackageState::Failed | ArkPackageState::Timeout | ArkPackageState::Ready => {
                rebuild_if_changed().await
            }
        }
    }
}

impl Ctx {
    const TIMEOUT_BUILDING: Option<Duration> =
        Some(Duration::from_secs(6 * 60 * 60 /* 6 hours */));
}

struct BuildArgs<'a> {
    manager: &'a Manager<Ctx>,
    data: &'a <Ctx as ::kiss_api::manager::Ctx>::Data,
}

async fn begin_build(BuildArgs { manager, data }: &BuildArgs<'_>) -> Result<Action, Error> {
    let name = data.name_any();
    let namespace = data.namespace_any();
    let job_name = PackageManager::job_name(&name);

    let timestamp: DateTime<Utc> = Utc::now();
    let metadata = ObjectMeta {
        labels: Some(job_labels(&name, Some(timestamp))),
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

    let home_dir = "/home/podman";
    let template_dir = format!("{home_dir}/src");
    let image_name = manager
        .ctx
        .app
        .get_image_name_from_package(&namespace, &package);

    let try_pull = if manager.ctx.pull {
        "podman pull --tls-verify=false {image_name}"
    } else {
        "false"
    };
    let command = format!(
        "{try_pull} || podman build --tag {image_name} {template_dir} && podman push --tls-verify=false {image_name}"
    );

    let job = Job {
        metadata: object_metadata,
        spec: Some(JobSpec {
            template: PodTemplateSpec {
                metadata: Some(metadata.clone()),
                spec: Some(PodSpec {
                    affinity: Some(Affinity {
                        node_affinity: Some(NodeAffinity {
                            preferred_during_scheduling_ignored_during_execution: Some(vec![
                                PreferredSchedulingTerm {
                                    weight: 1,
                                    preference: NodeSelectorTerm {
                                        match_expressions: Some(vec![NodeSelectorRequirement {
                                            key: "node-role.kubernetes.io/kiss-ephemeral-control-plane".into(),
                                            operator: "DoesNotExist".into(),
                                            values: None,
                                        }]),
                                        ..Default::default()
                                    },
                                },
                                PreferredSchedulingTerm {
                                    preference: NodeSelectorTerm {
                                        match_expressions: Some(vec![NodeSelectorRequirement {
                                            key: "node-role.kubernetes.io/kiss".into(),
                                            operator: "NotIn".into(),
                                            values: Some(vec!["ControlPlane".into()]),
                                        }]),
                                        ..Default::default()
                                    },
                                    weight: 2,
                                },
                                PreferredSchedulingTerm {
                                    preference: NodeSelectorTerm {
                                        match_expressions: Some(vec![NodeSelectorRequirement {
                                            key: "node-role.kubernetes.io/kiss".into(),
                                            operator: "In".into(),
                                            values: Some(vec!["Compute".into()]),
                                        }]),
                                        ..Default::default()
                                    },
                                    weight: 4,
                                },
                            ]),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    containers: vec![Container {
                        name: "build".into(),
                        image: Some("quay.io/podman/stable:latest".into()),
                        command: Some(vec!["bash".into(), "-c".into()]),
                        args: Some(vec![
                            command
                        ]),
                        env: Some(vec![EnvVar {
                            name: "HOME".into(),
                            value: Some(home_dir.into()),
                            value_from: None,
                        }]),
                        resources: Some(ResourceRequirements {
                            limits: Some(
                                // TODO: deploy fuse-device-plugin-daemonset
                                [("github.com/fuse", 1)]
                                    .iter()
                                    .map(|(k, v)| (k.to_string(), Quantity(v.to_string())))
                                    .collect(),
                            ),
                            ..Default::default()
                        }),
                        volume_mounts: Some(vec![
                            VolumeMount {
                                name: "template".into(),
                                mount_path: template_dir,
                                read_only: Some(true),
                                ..Default::default()
                            },
                            VolumeMount {
                                name: "user-local".into(),
                                mount_path: format!("{home_dir}/.local/share/containers"),
                                ..Default::default()
                            },
                        ]),
                        working_dir: Some(home_dir.into()),
                        ..Default::default()
                    }],
                    restart_policy: Some("OnFailure".into()),
                    security_context: Some(PodSecurityContext {
                        fs_group: Some(1000),
                        run_as_non_root: Some(true),
                        run_as_user: Some(1000),
                        ..Default::default()
                    }),
                    volumes: Some(vec![
                        Volume {
                            name: "template".into(),
                            config_map: Some(ConfigMapVolumeSource {
                                default_mode: Some(444),
                                name: Some(job_name),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        Volume {
                            name: "user-local".into(),
                            empty_dir: Some(EmptyDirVolumeSource {
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ]),
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        status: None,
    };

    if let Err(e) = super::try_delete_all::<Pod>(&manager.kube, &namespace, &name).await {
        warn!("failed to delete previous pods: {namespace} -> {name}: {e}");
        return Ok(Action::requeue(<Ctx as ::kiss_api::manager::Ctx>::FALLBACK));
    }

    match try_join!(
        force_create(&manager.kube, &name, &config_map),
        force_create(&manager.kube, &name, &job),
    ) {
        Ok(((), ())) => {
            info!("begin building: {namespace} -> {name}");
            let ctx = UpdateStateCtx {
                kube: &manager.kube,
                namespace: &namespace,
                name: &name,
                spec: &data.spec,
                state: ArkPackageState::Building,
                timestamp: Some(timestamp),
            };
            ctx.apply().await
        }
        Err(e) => {
            warn!("failed to begin building: {namespace} -> {name}: {e}");
            Ok(Action::requeue(<Ctx as ::kiss_api::manager::Ctx>::FALLBACK))
        }
    }
}

async fn cancel_build(
    manager: &Manager<Ctx>,
    data: &<Ctx as ::kiss_api::manager::Ctx>::Data,
    reason: &str,
) -> Result<Action, Error> {
    let name = data.name_any();
    let namespace = data.namespace_any();

    match try_join!(
        super::try_delete_all::<ConfigMap>(&manager.kube, &namespace, &name),
        super::try_delete_all::<Job>(&manager.kube, &namespace, &name),
    ) {
        Ok(((), ())) => {
            info!("canceled building ({reason}): {namespace} -> {name}");
            let ctx = UpdateStateCtx {
                kube: &manager.kube,
                namespace: &namespace,
                name: &name,
                spec: &data.spec,
                state: ArkPackageState::Timeout,
                timestamp: None,
            };
            ctx.apply().await
        }
        Err(e) => {
            warn!("failed to cancel building: {namespace} -> {name}: {e}");
            Ok(Action::requeue(<Ctx as ::kiss_api::manager::Ctx>::FALLBACK))
        }
    }
}

struct UpdateStateCtx<'a> {
    kube: &'a Client,
    namespace: &'a str,
    name: &'a str,
    spec: &'a ArkPackageSpec,
    state: ArkPackageState,
    timestamp: Option<DateTime<Utc>>,
}

impl<'a> UpdateStateCtx<'a> {
    async fn apply(&self) -> Result<Action, Error> {
        match self.try_apply().await {
            Ok(()) => match Ctx::TIMEOUT_BUILDING {
                Some(timeout) => Ok(Action::requeue(timeout)),
                None => Ok(Action::await_change()),
            },
            Err(e) => {
                let namespace = self.namespace;
                let name = self.name;
                info!("failed to update state: {namespace} -> {name}: {e}");

                Err(Error::Service(e.into()))
            }
        }
    }

    async fn try_apply(&self) -> Result<()> {
        let Self {
            kube,
            namespace,
            name,
            spec,
            state,
            timestamp,
        } = self;

        let api =
            Api::<<Ctx as ::kiss_api::manager::Ctx>::Data>::namespaced((*kube).clone(), namespace);
        let crd = <Ctx as ::kiss_api::manager::Ctx>::Data::api_resource();

        let patch = Patch::Merge(json!({
            "apiVersion": crd.api_version,
            "kind": crd.kind,
            "metadata": {
                "labels": job_labels(name, *timestamp),
            },
            "status": {
                "state": state,
                "spec": spec,
                "lastUpdated": timestamp.unwrap_or_else(Utc::now),
            },
        }));
        let pp = PatchParams::apply(<Ctx as ::kiss_api::manager::Ctx>::NAME);
        api.patch_status(name, &pp, &patch).await?;
        api.patch(name, &pp, &patch).await?;
        Ok(())
    }
}

async fn force_create<K>(kube: &Client, package_name: &str, data: &K) -> Result<()>
where
    K: Clone
        + fmt::Debug
        + Serialize
        + DeserializeOwned
        + Resource<Scope = NamespaceResourceScope>
        + ResourceExt,
    <K as Resource>::DynamicType: Default,
{
    let namespace = data.namespace_any();

    // delete last objects
    super::try_delete_all::<K>(kube, &namespace, package_name).await?;
    time::sleep(Duration::from_secs(1)).await;

    let api = Api::<K>::namespaced(kube.clone(), &namespace);
    let pp = PostParams {
        field_manager: Some(::ark_actor_kubernetes::consts::FIELD_MANAGER.into()),
        ..Default::default()
    };
    api.create(&pp, data).await.map(|_| ()).map_err(Into::into)
}

fn job_labels(name: &str, timestamp: Option<DateTime<Utc>>) -> BTreeMap<String, String> {
    let timestamp = timestamp.map(|timestamp| timestamp.timestamp_micros().to_string());

    [
        (
            ::ark_actor_kubernetes::consts::LABEL_BUILD_TIMESTAMP,
            timestamp.as_deref(),
        ),
        (
            ::ark_actor_kubernetes::consts::LABEL_PACKAGE_NAME,
            Some(name),
        ),
    ]
    .iter()
    .filter_map(|(k, v)| Some((k.to_string(), (*v)?.to_string())))
    .collect()
}
