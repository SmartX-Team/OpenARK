use std::{fs, io::Cursor, path::PathBuf, process::Stdio};

use ark_actor_api::{
    args::ContainerRuntimeKind,
    builder::{
        ApplicationBuilder, ApplicationBuilderArgs, ApplicationBuilderFactory, ApplicationDevice,
        ApplicationDeviceGpu, ApplicationDeviceGpuNvidia, ApplicationDeviceIpc,
        ApplicationEnvironmentVariable, ApplicationResource, ApplicationUserGroup,
        ApplicationVolume, ApplicationVolumeSource,
    },
    package::Package,
    runtime::ApplicationRuntime,
};
use ark_api::package::ArkUserSpec;
use ipis::{
    async_trait::async_trait,
    core::anyhow::{bail, Result},
    tokio::{
        io::{self, AsyncWriteExt},
        process::Command,
    },
};

use crate::template::Template;

pub(super) struct ContainerRuntimeManager {
    app: ApplicationRuntime<ContainerApplicationBuilderFactory>,
    kind: ContainerRuntimeKind,
    namespace: String,
    program: PathBuf,
}

impl ContainerRuntimeManager {
    pub(super) async fn try_new(
        kind: Option<ContainerRuntimeKind>,
        image_name_prefix: String,
    ) -> Result<Self> {
        let (kind, program) = ContainerRuntimeKind::parse(kind)?;
        Ok(Self {
            app: ApplicationRuntime::new(image_name_prefix),
            kind,
            namespace: {
                let hostname = ::gethostname::gethostname();
                let hostname = hostname.to_string_lossy();
                format!("localhost_{hostname}")
            },
            program,
        })
    }

    pub(super) async fn exists(&self, package: &Package) -> Result<bool> {
        let image_name = self
            .app
            .get_image_name_from_package(&self.namespace, package);

        let mut command = Command::new(&self.program);
        let command = match &self.kind {
            ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => command
                .arg("image")
                .arg("ls")
                .arg("--quiet")
                .arg(image_name),
        };

        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .output()
            .await
            .map(|output| !output.stdout.is_empty())
            .map_err(Into::into)
    }

    pub(super) async fn build(&self, template: &Template) -> Result<()> {
        let name = &template.name;
        let mut text = Cursor::new(&template.text);

        let image_name = self
            .app
            .get_image_name(&self.namespace, name, &template.version);

        let mut command = Command::new(&self.program);
        let command = match &self.kind {
            ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => {
                command.arg("build").arg("--tag").arg(image_name).arg("-")
            }
        };

        let mut process = command
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        {
            let mut stdin = process.stdin.take().unwrap();
            io::copy(&mut text, &mut stdin).await?;
            stdin.shutdown().await?;
        }

        if process.wait().await?.success() {
            Ok(())
        } else {
            bail!("failed to build package: {name:?}")
        }
    }

    pub(super) async fn remove(&self, package: &Package) -> Result<()> {
        let image_name = self
            .app
            .get_image_name_from_package(&self.namespace, package);

        let mut command = Command::new(&self.program);
        let command = match &self.kind {
            ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => command
                .arg("image")
                .arg("rm")
                .arg("--force")
                .arg(image_name),
        };

        if command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .status()
            .await?
            .success()
        {
            Ok(())
        } else {
            let name = &package.name;
            bail!("failed to delete package: {name:?}")
        }
    }

    pub(super) async fn run(
        &self,
        package: &Package,
        command_line_arguments: &[String],
    ) -> Result<()> {
        let args = ContainerApplicationBuilderArgs {
            manager: self,
            name: &package.name,
        };
        let node_name = None;
        self.app
            .spawn(
                args,
                &self.namespace,
                node_name,
                package,
                command_line_arguments,
            )
            .await
    }
}

#[derive(Default)]
struct ContainerApplicationBuilderFactory;

#[async_trait]
impl<'args> ApplicationBuilderFactory<'args> for ContainerApplicationBuilderFactory {
    type Args = ContainerApplicationBuilderArgs<'args>;
    type Builder = ContainerApplicationBuilder<'args>;

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
        Ok(ContainerApplicationBuilder {
            command: match &args.manager.kind {
                ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => {
                    let ArkUserSpec { uid, gid, .. } = user;

                    let mut command = Command::new(&args.manager.program);
                    command
                        .arg("run")
                        .arg("--rm")
                        .arg("--group-add")
                        .arg(gid.to_string())
                        .arg("--user")
                        .arg(uid.to_string());
                    command
                }
            },
            args,
            command_line_arguments,
            image_name,
        })
    }
}

struct ContainerApplicationBuilder<'args> {
    args: ContainerApplicationBuilderArgs<'args>,
    command: Command,
    command_line_arguments: &'args [String],
    image_name: String,
}

struct ContainerApplicationBuilderArgs<'args> {
    manager: &'args ContainerRuntimeManager,
    name: &'args str,
}

#[async_trait]
impl<'args> ApplicationBuilder for ContainerApplicationBuilder<'args> {
    fn add(&mut self, resource: ApplicationResource) -> Result<()> {
        match &self.args.manager.kind {
            ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => match resource {
                ApplicationResource::Box(_) => Ok(()),
                ApplicationResource::Device(device) => match device {
                    ApplicationDevice::Gpu(gpu) => match gpu {
                        ApplicationDeviceGpu::Nvidia(nvidia) => match nvidia {
                            ApplicationDeviceGpuNvidia::All => {
                                self.command.arg("--gpus");
                                self.command.arg("all");
                                Ok(())
                            }
                        },
                    },
                    ApplicationDevice::Ipc(ipc) => match ipc {
                        ApplicationDeviceIpc::Host => {
                            self.command.arg("--ipc");
                            self.command.arg("host");
                            Ok(())
                        }
                    },
                },
                ApplicationResource::EnvironmentVariable(ApplicationEnvironmentVariable {
                    key,
                    value,
                }) => {
                    self.command.arg("--env");
                    self.command.arg(format!("{key}={value}"));
                    Ok(())
                }
                ApplicationResource::NodeName(_) => Ok(()),
                ApplicationResource::UserGroup(group) => match group {
                    ApplicationUserGroup::Gid(gid) => {
                        self.command.arg("--group-add");
                        self.command.arg(gid.to_string());
                        Ok(())
                    }
                    ApplicationUserGroup::Name(name) => {
                        self.command.arg("--group-add");
                        self.command.arg(name);
                        Ok(())
                    }
                },
                ApplicationResource::Volume(ApplicationVolume {
                    src,
                    dst_path,
                    read_only,
                }) => match src {
                    ApplicationVolumeSource::HostPath(src_path) => {
                        let src_path = src_path.unwrap_or(dst_path);
                        let permission = if read_only { "ro" } else { "" };

                        self.command.arg("--volume");
                        self.command
                            .arg(format!("{src_path}:{dst_path}:{permission}"));
                        Ok(())
                    }
                    ApplicationVolumeSource::UserHome(src_path) => {
                        let home = ::std::env::var("HOME")?; // TODO: enable to use virtualized home
                        let src_path = src_path.unwrap_or(dst_path);
                        let src_path = format!("{home}/{src_path}");
                        let permission = if read_only { "ro" } else { "" };

                        // make a user-level directory if not exists
                        fs::create_dir_all(&src_path)?;

                        self.command.arg("--volume");
                        self.command
                            .arg(format!("{src_path}:{dst_path}:{permission}"));
                        Ok(())
                    }
                },
            },
        }
    }

    async fn spawn(mut self) -> Result<()> {
        match &self.args.manager.kind {
            ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => {
                self.command
                    .arg(&self.image_name)
                    .args(self.command_line_arguments);
            }
        }

        if self
            .command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .status()
            .await?
            .success()
        {
            Ok(())
        } else {
            let name = &self.args.name;
            bail!("failed to run package: {name:?}")
        }
    }
}
