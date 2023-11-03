#[cfg(feature = "oci")]
mod oci;

use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::Client;
use straw_api::{
    pipe::{StrawNode, StrawPipe},
    plugin::{Plugin, PluginBuilder, PluginContext},
};

pub struct StrawSession {
    builders: Vec<Box<dyn Send + Sync + PluginBuilder>>,
    kube: Client,
    namespace: Option<String>,
}

impl StrawSession {
    pub fn new(kube: Client, namespace: Option<String>) -> Self {
        Self {
            builders: vec![
                #[cfg(feature = "ai")]
                Box::new(::dash_pipe_function_ai::plugin::PluginBuilder::new()),
                #[cfg(feature = "oci")]
                Box::new(self::oci::PluginBuilder::new()),
            ],
            kube,
            namespace,
        }
    }

    pub fn add_plugin(&mut self, builder: impl Into<Box<dyn Send + Sync + PluginBuilder>>) {
        self.builders.push(builder.into())
    }

    pub async fn create(&self, ctx: &PluginContext, pipe: &StrawPipe) -> Result<()> {
        pipe.straw
            .iter()
            .map(|node| self.create_node(ctx, node))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    async fn create_node(&self, ctx: &PluginContext, node: &StrawNode) -> Result<()> {
        self.load_plugin(node)
            .await?
            .create(self.kube.clone(), self.namespace.as_deref(), ctx, node)
            .await
    }

    pub async fn delete(&self, pipe: &StrawPipe) -> Result<()> {
        pipe.straw
            .iter()
            .map(|node| self.delete_node(node))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    async fn delete_node(&self, node: &StrawNode) -> Result<()> {
        self.load_plugin(node)
            .await?
            .delete(self.kube.clone(), self.namespace.as_deref(), node)
            .await
    }

    pub async fn exists(&self, pipe: &StrawPipe) -> Result<bool> {
        pipe.straw
            .iter()
            .map(|node| self.exists_node(node))
            .collect::<FuturesUnordered<_>>()
            .try_any(|exists| async move { exists })
            .await
    }

    async fn exists_node(&self, node: &StrawNode) -> Result<bool> {
        self.load_plugin(node)
            .await?
            .exists(self.kube.clone(), self.namespace.as_deref(), node)
            .await
    }

    async fn load_plugin(&self, node: &StrawNode) -> Result<Box<dyn Send + Plugin>> {
        let url = &node.src;
        self.builders
            .iter()
            .find_map(|builder| builder.try_build(url))
            .ok_or_else(|| anyhow!("unsupported straw url: {url}"))
    }
}
