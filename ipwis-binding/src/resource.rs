use ipis::{async_trait::async_trait, core::anyhow::Result};
use ipwis_kernel_common::{resource::ResourceId, task::TaskConstraints};

pub struct ResourceManager {}

#[async_trait]
impl ::ipwis_kernel_common::resource::ResourceManager for ResourceManager {
    async fn alloc(&self, constraints: &TaskConstraints) -> Result<Option<ResourceId>> {
        todo!()
    }
}
