use anyhow::Result;
use ark_api::package::ArkUserSpec;
use async_trait::async_trait;
use kiss_api::r#box::BoxGroupRole;

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
    pub image_name: String,
    pub user: &'a ArkUserSpec,
}

#[async_trait]
pub trait ApplicationBuilder {
    fn add(&mut self, resource: ApplicationResource) -> Result<()>;

    async fn spawn(self, sync: bool) -> Result<()>;
}

pub enum ApplicationResource<'a> {
    Box(BoxGroupRole),
    Device(ApplicationDevice<'a>),
    EnvironmentVariable(ApplicationEnvironmentVariable<'a>),
    NodeName(&'a str),
    UserGroup(ApplicationUserGroup<'a>),
    Volume(ApplicationVolume<'a>),
}

pub enum ApplicationDevice<'a> {
    Gpu(ApplicationDeviceGpu),
    Ipc(ApplicationDeviceIpc),
    Path(ApplicationDevicePath<'a>),
}

pub struct ApplicationDevicePath<'a> {
    pub src: &'a str,
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
