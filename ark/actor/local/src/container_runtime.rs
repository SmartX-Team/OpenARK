use std::{ffi::OsStr, io::Cursor, path::PathBuf, process::Stdio};

use ark_actor_api::{args::ContainerRuntimeKind, package::Package};
use ark_api::package::{ArkPermissionKind, ArkUserSpec};
use ipis::{
    core::anyhow::{bail, Result},
    tokio::{
        io::{self, AsyncWriteExt},
        process::Command,
    },
};

use crate::template::Template;

pub(super) struct ContainerRuntimeManager {
    kind: ContainerRuntimeKind,
    name_prefix: String,
    runtime: PathBuf,
}

impl ContainerRuntimeManager {
    pub(super) async fn try_new(
        kind: Option<ContainerRuntimeKind>,
        name_prefix: String,
    ) -> Result<Self> {
        let (kind, runtime) = ContainerRuntimeKind::parse(kind)?;
        Ok(Self {
            kind,
            name_prefix,
            runtime,
        })
    }

    pub(super) async fn exists(&self, package: &Package) -> Result<bool> {
        let image_name = self.get_image_name_from_package(package)?;

        let mut command = Command::new(&self.runtime);
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

        let image_name = self.get_image_name(name, &template.version);

        let mut command = Command::new(&self.runtime);
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
        let image_name = self.get_image_name_from_package(package)?;

        let mut command = Command::new(&self.runtime);
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

    pub(super) async fn run<I, S>(&self, package: &Package, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let image_name = self.get_image_name_from_package(package)?;

        let mut command = Command::new(&self.runtime);
        let command = match &self.kind {
            ContainerRuntimeKind::Docker | ContainerRuntimeKind::Podman => {
                let ArkUserSpec { uid, gid, .. } = package.user()?;

                command
                    .arg("run")
                    .arg("--rm")
                    .arg("--group-add")
                    .arg(gid.to_string())
                    .arg("--user")
                    .arg(uid.to_string());

                for permission in package.permissions()? {
                    match &permission.name {
                        ArkPermissionKind::Audio => {
                            command
                                .arg("--group-add")
                                .arg("audio")
                                .arg("--volume")
                                .arg("/dev/snd:/dev/snd:ro")
                                .arg("--volume")
                                .arg("/etc/pulse:/etc/pulse:ro");
                        }
                        ArkPermissionKind::Graphics => {
                            let home = ::std::env::var("HOME")?;

                            command
                                .arg("--gpus")
                                .arg("all")
                                .arg("--group-add")
                                .arg("render")
                                .arg("--group-add")
                                .arg("video")
                                .arg("--env")
                                .arg("DISPLAY=:0")
                                .arg("--env")
                                .arg("NVIDIA_DRIVER_CAPABILITIES=all")
                                .arg("--env")
                                .arg("NVIDIA_VISIBLE_DEVICES=all")
                                .arg("--env")
                                .arg(format!("XDG_RUNTIME_DIR=/run/user/{uid}"))
                                .arg("--ipc")
                                .arg("host")
                                .arg("--volume")
                                .arg("/dev/dri:/dev/dri:ro")
                                .arg("--volume")
                                .arg(format!("{home}/indocker:/home/user"))
                                .arg("--volume")
                                .arg(format!("{home}/.local/share:/home/user/share:ro"))
                                .arg("--volume")
                                .arg("/usr/share/egl/egl_external_platform.d:/usr/share/egl/egl_external_platform.d:ro")
                                .arg("--volume")
                                .arg("/usr/share/glvnd/egl_vendor.d:/usr/share/glvnd/egl_vendor.d:ro")
                                .arg("--volume")
                                .arg("/usr/share/vulkan/icd.d:/usr/share/vulkan/icd.d:ro")
                                .arg("--volume")
                                .arg("/run/dbus:/run/dbus:ro")
                                .arg("--volume")
                                .arg(format!("/run/user/{uid}:/run/user/{uid}"))
                                .arg("--volume")
                                .arg("/tmp/.ICE-unix:/tmp/.ICE-unix:ro")
                                .arg("--volume")
                                .arg("/tmp/.X11-unix:/tmp/.X11-unix:ro");
                        }
                    }
                }

                command.arg(image_name).args(args)
            }
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
            bail!("failed to run package: {name:?}")
        }
    }

    fn get_image_name(&self, name: &str, version: &str) -> String {
        let name_prefix = &self.name_prefix;
        format!("{name_prefix}{name}:{version}")
    }

    fn get_image_name_from_package(&self, package: &Package) -> Result<String> {
        package
            .version()
            .map(|version| self.get_image_name(&package.name, version))
    }
}
