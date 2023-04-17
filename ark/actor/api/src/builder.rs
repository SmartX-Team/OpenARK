use ark_api::package::ArkUserSpec;
use ipis::{async_trait::async_trait, core::anyhow::Result};

#[async_trait]
pub trait ApplicationBuilderFactory<'args> {
    type Args;
    type Builder: ApplicationBuilder;

    async fn create_builder<'builder>(
        &self,
        args: <Self as ApplicationBuilderFactory<'args>>::Args,
        builder_args: ApplicationBuilderArgs<'builder>,
    ) -> Result<<Self as ApplicationBuilderFactory<'args>>::Builder>
    where
        'builder: 'args;
}

pub struct ApplicationBuilderArgs<'a> {
    pub command_line_arguments: &'a [String],
    pub user: &'a ArkUserSpec,
}

#[async_trait]
pub trait ApplicationBuilder {
    fn add(&mut self, resource: ApplicationResource) -> Result<()>;

    async fn spawn(&mut self) -> Result<()>;
}

pub enum ApplicationResource<'a> {
    Device(ApplicationDevice),
    EnvironmentVariable(ApplicationEnvironmentVariable<'a>),
    UserGroup(ApplicationUserGroup<'a>),
    Volume(ApplicationVolume<'a>),
}

pub enum ApplicationDevice {
    Gpu(ApplicationDeviceGpu),
    Ipc(ApplicationDeviceIpc),
}

pub enum ApplicationDeviceGpu {
    Nvidia(ApplicationDeviceGpuNvidia),
}

pub enum ApplicationDeviceGpuNvidia {
    All,
}

pub enum ApplicationDeviceIpc {
    Host,
}

pub struct ApplicationEnvironmentVariable<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

pub enum ApplicationUserGroup<'a> {
    Gid(usize),
    Name(&'a str),
}

pub struct ApplicationVolume<'a> {
    pub src: ApplicationVolumeSource<'a>,
    pub dst_path: &'a str,
    pub read_only: bool,
}

pub enum ApplicationVolumeSource<'a> {
    HostPath(Option<&'a str>),
    UserHome(Option<&'a str>),
}
