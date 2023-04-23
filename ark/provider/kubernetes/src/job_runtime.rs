use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use ark_api::{package::ArkUserSpec, NamespaceAny};
use ark_provider_api::{
    builder::{
        ApplicationBuilder, ApplicationBuilderArgs, ApplicationBuilderFactory, ApplicationDevice,
        ApplicationDeviceGpu, ApplicationDeviceGpuNvidia, ApplicationDeviceIpc,
        ApplicationEnvironmentVariable, ApplicationResource, ApplicationVolume,
        ApplicationVolumeSource,
    },
    package::Package,
};
use ipis::{
    async_trait::async_trait,
    core::anyhow::Result,
    log::{info, warn},
    tokio::{join, spawn, time::sleep},
};
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{
            Affinity, Container, EnvVar, HostPathVolumeSource, LocalObjectReference, NodeAffinity,
            NodeSelectorRequirement, NodeSelectorTerm, Pod, PodSecurityContext, PodSpec,
            PodTemplateSpec, Volume, VolumeMount,
        },
    },
    chrono::Utc,
};
use kube::{
    api::{DeleteParams, ListParams, PostParams},
    core::ObjectMeta,
    Api, Client,
};

use crate::consts::JobKind;

#[derive(Default)]
pub(crate) struct JobApplicationBuilderFactory;

#[async_trait]
impl<'args> ApplicationBuilderFactory<'args> for JobApplicationBuilderFactory {
    type Args = JobApplicationBuilderArgs<'args>;
    type Builder = JobApplicationBuilder<'args>;

    async fn create_builder<'builder>(
        &self,
        args: <Self as ApplicationBuilderFactory<'args>>::Args,
        ApplicationBuilderArgs {
            command_line_arguments,
            image_name,
            user,
        }: ApplicationBuilderArgs<'builder>,
    ) -> Result<<Self as ApplicationBuilderFactory<'args>>::Builder>
    where
        'builder: 'args,
    {
        let ArkUserSpec { uid, gid, .. } = user;
        let job_kind = JobKind::Run.to_string();

        Ok(JobApplicationBuilder {
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(
                        [
                            (crate::consts::LABEL_JOB_KIND, &job_kind),
                            (crate::consts::LABEL_PACKAGE_NAME, &args.package.name),
                        ]
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    ),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: args.package.name.clone(),
                        // command: Some(vec!["/usr/bin/env".into(), "firefox".into()]),
                        args: Some(command_line_arguments.to_vec()),
                        image: Some(image_name),
                        image_pull_policy: Some("Always".into()),
                        ..Default::default()
                    }],
                    // TODO: deploy user registry accounts on VINE
                    image_pull_secrets: Some(vec![LocalObjectReference {
                        name: Some(crate::consts::IMAGE_PULL_SECRET_NAME.into()),
                    }]),
                    restart_policy: Some("Never".into()),
                    security_context: Some(PodSecurityContext {
                        fs_group: Some(*uid),
                        run_as_group: Some(*gid),
                        run_as_non_root: Some(true),
                        run_as_user: Some(*uid),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            },
            args,
        })
    }
}

pub(crate) struct JobApplicationBuilder<'args> {
    args: JobApplicationBuilderArgs<'args>,
    template: PodTemplateSpec,
}

impl<'args> JobApplicationBuilder<'args> {
    fn affinity(&mut self) -> &mut Affinity {
        self.pod().affinity.get_or_insert_with(Default::default)
    }

    fn container(&mut self) -> &mut Container {
        self.pod().containers.first_mut().unwrap()
    }

    fn env(&mut self) -> &mut Vec<EnvVar> {
        self.container().env.get_or_insert_with(Default::default)
    }

    fn namespace(&self) -> String {
        self.args.package.resource.namespace_any()
    }

    fn node_affinity(&mut self) -> &mut NodeAffinity {
        self.affinity()
            .node_affinity
            .get_or_insert_with(Default::default)
    }

    fn node_selector_terms_required(&mut self) -> &mut Vec<NodeSelectorTerm> {
        &mut self
            .node_affinity()
            .required_during_scheduling_ignored_during_execution
            .get_or_insert_with(Default::default)
            .node_selector_terms
    }

    fn pod(&mut self) -> &mut PodSpec {
        self.template.spec.get_or_insert_with(Default::default)
    }

    fn volume_mounts(&mut self) -> &mut Vec<VolumeMount> {
        self.container()
            .volume_mounts
            .get_or_insert_with(Default::default)
    }

    fn volumes(&mut self) -> &mut Vec<Volume> {
        self.pod().volumes.get_or_insert_with(Default::default)
    }
}

pub(crate) struct JobApplicationBuilderArgs<'args> {
    pub kube: &'args Client,
    pub package: &'args Package,
}

#[async_trait]
impl<'args> ApplicationBuilder for JobApplicationBuilder<'args> {
    fn add(&mut self, resource: ApplicationResource) -> Result<()> {
        match resource {
            ApplicationResource::Box(r#box) => {
                self.node_selector_terms_required().push(NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec![r#box.to_string()]),
                    }]),
                    ..Default::default()
                });
                Ok(())
            }
            ApplicationResource::Device(device) => match device {
                ApplicationDevice::Gpu(gpu) => match gpu {
                    ApplicationDeviceGpu::Nvidia(nvidia) => match nvidia {
                        ApplicationDeviceGpuNvidia::All => {
                            self.add(ApplicationResource::Volume(ApplicationVolume {
                                src: ApplicationVolumeSource::HostPath(None),
                                dst_path: "/dev",
                                read_only: true,
                            }))
                        }
                    },
                },
                ApplicationDevice::Ipc(ipc) => match ipc {
                    ApplicationDeviceIpc::Host => {
                        self.pod().host_ipc.replace(true);
                        Ok(())
                    }
                },
            },
            ApplicationResource::EnvironmentVariable(ApplicationEnvironmentVariable {
                key,
                value,
            }) => {
                self.env().push(EnvVar {
                    name: key.to_string(),
                    value: Some(value.to_string()),
                    value_from: None,
                });
                Ok(())
            }
            ApplicationResource::NodeName(node_name) => {
                self.node_selector_terms_required().push(NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "kubernetes.io/hostname".into(),
                        operator: "In".into(),
                        values: Some(vec![node_name.to_string()]),
                    }]),
                    ..Default::default()
                });
                Ok(())
            }
            ApplicationResource::UserGroup(_) => Ok(()),
            ApplicationResource::Volume(ApplicationVolume {
                src,
                dst_path,
                read_only,
            }) => match src {
                ApplicationVolumeSource::HostPath(src_path) => {
                    let name = format!("ark-volume-{}", self.volume_mounts().len());

                    self.volume_mounts().push(VolumeMount {
                        name: name.clone(),
                        mount_path: dst_path.to_string(),
                        read_only: Some(read_only),
                        ..Default::default()
                    });
                    self.volumes().push(Volume {
                        name,
                        host_path: Some(HostPathVolumeSource {
                            path: src_path.unwrap_or(dst_path).to_string(),
                            type_: None,
                        }),
                        ..Default::default()
                    });
                    Ok(())
                }
                ApplicationVolumeSource::UserHome(src_path) => {
                    let namespace = self.namespace();
                    let src_path = src_path.unwrap_or_default();
                    let src_path =
                        format!("/opt/vdi/tenants/remote/{namespace}/desktop/{src_path}"); // TODO: implement it!

                    self.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(Some(&src_path)),
                        dst_path,
                        read_only,
                    }))
                }
            },
        }
    }

    async fn spawn(self, sync: bool) -> Result<()> {
        let name = {
            let name = &self.args.package.name;
            let timestamp = Utc::now().timestamp_nanos();
            format!("{name}-{timestamp}")
        };
        let namespace = self.namespace();

        let job = Job {
            metadata: ObjectMeta {
                name: Some(name.clone()),
                namespace: Some(self.namespace()),
                ..self.template.metadata.clone().unwrap()
            },
            spec: Some(JobSpec {
                backoff_limit: Some(0),
                ttl_seconds_after_finished: Some(30),
                template: self.template,
                ..Default::default()
            }),
            status: None,
        };

        let api_job = Api::<Job>::default_namespaced(self.args.kube.clone());
        let api_pod = Api::<Pod>::default_namespaced(self.args.kube.clone());

        #[derive(Default)]
        struct CompleteFlag(AtomicBool);

        impl CompleteFlag {
            fn is_completed(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }

            fn complete(&self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let complete_flag: Arc<CompleteFlag> = Default::default();

        let delete_all = spawn({
            let complete_flag = complete_flag.clone();

            let delete_job = {
                let name = name.clone();
                let namespace = namespace.clone();

                let api = api_job.clone();
                let dp = DeleteParams::default();

                async move {
                    info!("Removing job ({namespace}/{name})...");

                    match api.delete(&name, &dp).await {
                        Ok(_) => (),
                        Err(e) => warn!("failed to terminate job: {name}: {e}"),
                    }
                }
            };

            let delete_pods = {
                let name = name.clone();
                let namespace = namespace.clone();

                let api = api_pod.clone();
                let dp = DeleteParams::default();
                let lp = ListParams {
                    label_selector: Some(format!("job-name={name}")),
                    ..Default::default()
                };

                async move {
                    info!("Removing pod ({namespace}/{name})...");

                    match api.delete_collection(&dp, &lp).await {
                        Ok(_) => (),
                        Err(e) => warn!("failed to terminate pods: {name}: {e}"),
                    }
                }
            };

            async move {
                while !complete_flag.is_completed() {
                    sleep(Duration::from_millis(100)).await;
                }

                join!(delete_job, delete_pods);
            }
        });

        if sync {
            let complete_flag = complete_flag.clone();

            ::ctrlc::set_handler(move || complete_flag.complete())?;
        }

        let create = async move {
            let pp = PostParams {
                field_manager: Some(crate::consts::FIELD_MANAGER.into()),
                ..Default::default()
            };
            api_job.create(&pp, &job).await?;

            if sync {
                let skip_if_not_exists = true;
                crate::show_logs(&api_pod, &namespace, &name, skip_if_not_exists).await
            } else {
                Ok(())
            }
        };

        let result = create.await;
        complete_flag.complete();
        delete_all.await.ok();
        result
    }
}
