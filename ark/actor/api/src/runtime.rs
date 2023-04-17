use ark_api::package::{ArkPermissionKind, ArkUserSpec};
use ipis::core::anyhow::Result;

use crate::{
    builder::{
        ApplicationBuilder, ApplicationBuilderArgs, ApplicationBuilderFactory, ApplicationDevice,
        ApplicationDeviceGpu, ApplicationDeviceGpuNvidia, ApplicationDeviceIpc,
        ApplicationEnvironmentVariable, ApplicationResource, ApplicationUserGroup,
        ApplicationVolume, ApplicationVolumeSource,
    },
    package::Package,
};

#[derive(Default)]
pub struct ApplicationRuntime<Builder> {
    builder: Builder,
}

impl<'args, Builder> ApplicationRuntime<Builder>
where
    Builder: ApplicationBuilderFactory<'args>,
{
    pub async fn spawn<'package, 'command>(
        &self,
        args: <Builder as ApplicationBuilderFactory<'args>>::Args,
        Package { name, resource }: &'package Package,
        command_line_arguments: &'command [String],
    ) -> Result<()>
    where
        'package: 'args,
        'command: 'args,
    {
        let ArkUserSpec {
            name: username,
            uid,
            ..
        } = &resource.spec.user;

        let mut builder = self
            .builder
            .create_builder(
                args,
                ApplicationBuilderArgs {
                    command_line_arguments,
                    user: &resource.spec.user,
                },
            )
            .await?;

        for permission in &resource.spec.permissions {
            match &permission.name {
                ArkPermissionKind::Audio => {
                    builder.add(ApplicationResource::UserGroup(ApplicationUserGroup::Name(
                        "audio",
                    )))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/dev/snd",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/etc/pulse",
                        read_only: true,
                    }))?;
                }
                ArkPermissionKind::Graphics => {
                    builder.add(ApplicationResource::Device(ApplicationDevice::Gpu(
                        ApplicationDeviceGpu::Nvidia(ApplicationDeviceGpuNvidia::All),
                    )))?;
                    builder.add(ApplicationResource::Device(ApplicationDevice::Ipc(
                        ApplicationDeviceIpc::Host,
                    )))?;
                    builder.add(ApplicationResource::UserGroup(ApplicationUserGroup::Name(
                        "render",
                    )))?;
                    builder.add(ApplicationResource::UserGroup(ApplicationUserGroup::Name(
                        "video",
                    )))?;
                    builder.add(ApplicationResource::EnvironmentVariable(
                        ApplicationEnvironmentVariable {
                            key: "DISPLAY",
                            value: ":0",
                        },
                    ))?;
                    builder.add(ApplicationResource::EnvironmentVariable(
                        ApplicationEnvironmentVariable {
                            key: "NVIDIA_DRIVER_CAPABILITIES",
                            value: "all",
                        },
                    ))?;
                    builder.add(ApplicationResource::EnvironmentVariable(
                        ApplicationEnvironmentVariable {
                            key: "NVIDIA_VISIBLE_DEVICES",
                            value: "all",
                        },
                    ))?;
                    builder.add(ApplicationResource::EnvironmentVariable(
                        ApplicationEnvironmentVariable {
                            key: "XDG_RUNTIME_DIR",
                            value: &format!("/run/user/{uid}"),
                        },
                    ))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/dev/dri",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::UserHome(Some(&format!(
                            "./.local/share/ark/{name}/"
                        ))),
                        dst_path: &format!("/home/{username}"),
                        read_only: false,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/usr/share/egl/egl_external_platform.d",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/usr/share/glvnd/egl_vendor.d",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/usr/share/vulkan/icd.d",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/run/dbus",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: &format!("/run/user/{uid}"),
                        read_only: false,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/tmp/.ICE-unix",
                        read_only: true,
                    }))?;
                    builder.add(ApplicationResource::Volume(ApplicationVolume {
                        src: ApplicationVolumeSource::HostPath(None),
                        dst_path: "/tmp/.X11-unix",
                        read_only: true,
                    }))?;
                }
            }
        }
        builder.spawn().await
    }
}
