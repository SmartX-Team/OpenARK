use anyhow::Result;
use async_trait::async_trait;
use kube::Client;

pub trait PluginBuilder {
    fn try_build(&self, scheme: &str) -> Result<Option<Box<dyn Plugin>>>;
}

#[async_trait]
pub trait Plugin {
    async fn create(&self, client: &Client) -> Result<()>;

    async fn delete(&self, client: &Client) -> Result<()>;
}
