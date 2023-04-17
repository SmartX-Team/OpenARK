use ark_actor_api::{
    builder::{
        ApplicationBuilder, ApplicationBuilderArgs, ApplicationBuilderFactory, ApplicationDevice,
        ApplicationDeviceGpu, ApplicationDeviceGpuNvidia, ApplicationDeviceIpc,
        ApplicationEnvironmentVariable, ApplicationResource, ApplicationVolume,
        ApplicationVolumeSource,
    },
    package::Package,
};
use ark_api::package::ArkUserSpec;
use ipis::{async_trait::async_trait, core::anyhow::Result};
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{
            Container, EnvVar, HostPathVolumeSource, PodSecurityContext, PodSpec, PodTemplateSpec,
            Volume, VolumeMount,
        },
    },
    chrono::Utc,
};
use kube::{api::PostParams, core::ObjectMeta, Api, Client, ResourceExt};

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

        Ok(JobApplicationBuilder {
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(
                        [("name", &args.package.name)]
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                    ),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: args.package.name.clone(),
                        command: Some(command_line_arguments.to_vec()),
                        image: Some(image_name),
                        image_pull_policy: Some("always".into()),
                        ..Default::default()
                    }],
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
    fn container(&mut self) -> &mut Container {
        self.pod().containers.get_mut(0).unwrap()
    }

    fn env(&mut self) -> &mut Vec<EnvVar> {
        self.container().env.as_mut().unwrap()
    }

    fn namespace(&self) -> String {
        self.args
            .package
            .resource
            .namespace()
            .unwrap_or_else(|| "default".into())
    }

    fn pod(&mut self) -> &mut PodSpec {
        self.template.spec.as_mut().unwrap()
    }

    fn volume_mounts(&mut self) -> &mut Vec<VolumeMount> {
        self.container().volume_mounts.as_mut().unwrap()
    }

    fn volumes(&mut self) -> &mut Vec<Volume> {
        self.pod().volumes.as_mut().unwrap()
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
            ApplicationResource::UserGroup(_) => Ok(()),
            ApplicationResource::Volume(ApplicationVolume {
                src,
                dst_path,
                read_only,
            }) => match src {
                ApplicationVolumeSource::HostPath(src_path) => {
                    let name = format!("volume-{}", self.volume_mounts().len());

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

    async fn spawn(self) -> Result<()> {
        let name = &self.args.package.name;
        let timestamp = Utc::now().timestamp_nanos();

        let job = Job {
            metadata: ObjectMeta {
                name: Some(format!("{name}-{timestamp}")),
                namespace: Some(self.namespace()),
                ..Default::default()
            },
            spec: Some(JobSpec {
                backoff_limit: Some(0),
                ttl_seconds_after_finished: Some(0),
                template: self.template,
                ..Default::default()
            }),
            status: None,
        };

        let api = Api::<Job>::default_namespaced(self.args.kube.clone());
        let pp = PostParams {
            field_manager: Some(super::FIELD_MANAGER.into()),
            ..Default::default()
        };
        api.create(&pp, &job).await.map(|_| ()).map_err(Into::into)
    }
}
