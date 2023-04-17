use ark_actor_api::package::Package;
use ipis::core::anyhow::Result;
use kube::Client;

pub struct JobRuntimeBuilder<'args, 'kube, 'package> {
    pub args: &'args [String],
    pub kube: &'kube Client,
    pub package: &'package Package,
}

impl<'args, 'kube, 'package> JobRuntimeBuilder<'args, 'kube, 'package> {
    pub async fn spawn(&self) -> Result<()> {
        todo!()
    }
}
