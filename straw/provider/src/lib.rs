use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::Client;
use straw_api::{
    function::{StrawFunction, StrawNode},
    plugin::{Plugin, PluginBuilder, PluginContext},
};
use tracing::{instrument, Level};

pub struct StrawSession {
    builders: Vec<Box<dyn Send + Sync + PluginBuilder>>,
    kube: Client,
    namespace: Option<String>,
}

impl StrawSession {
    pub fn new(kube: Client, namespace: Option<String>) -> Self {
        Self {
            builders: vec![
                #[cfg(feature = "oci")]
                Box::new(::straw_provider_oci::PluginBuilder::new()),
                #[cfg(feature = "python")]
                Box::new(::straw_provider_python::PluginBuilder::new()),
            ],
            kube,
            namespace,
        }
    }

    pub fn add_plugin(&mut self, builder: impl Into<Box<dyn Send + Sync + PluginBuilder>>) {
        self.builders.push(builder.into())
    }

    #[instrument(level = Level::INFO, skip(self, ctx, function), err(Display))]
    pub async fn create(&self, ctx: &PluginContext, function: &StrawFunction) -> Result<()> {
        function
            .straw
            .iter()
            .map(|node| self.create_node(ctx, node))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    #[instrument(level = Level::INFO, skip(self, ctx, node), fields(node.name = %node.name, node.src = %node.src), err(Display))]
    async fn create_node(&self, ctx: &PluginContext, node: &StrawNode) -> Result<()> {
        self.load_plugin(node)
            .await?
            .create(self.kube.clone(), self.namespace.as_deref(), ctx, node)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, function), err(Display))]
    pub async fn delete(&self, function: &StrawFunction) -> Result<()> {
        function
            .straw
            .iter()
            .map(|node| self.delete_node(node))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node.name = %node.name, node.src = %node.src), err(Display))]
    async fn delete_node(&self, node: &StrawNode) -> Result<()> {
        self.load_plugin(node)
            .await?
            .delete(self.kube.clone(), self.namespace.as_deref(), node)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, function), err(Display))]
    pub async fn exists(&self, function: &StrawFunction) -> Result<bool> {
        function
            .straw
            .iter()
            .map(|node| self.exists_node(node))
            .collect::<FuturesUnordered<_>>()
            .try_any(|exists| async move { exists })
            .await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node.name = %node.name, node.src = %node.src), err(Display))]
    async fn exists_node(&self, node: &StrawNode) -> Result<bool> {
        self.load_plugin(node)
            .await?
            .exists(self.kube.clone(), self.namespace.as_deref(), node)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node.name = %node.name, node.src = %node.src), err(Display))]
    async fn load_plugin(&self, node: &StrawNode) -> Result<Box<dyn Send + Plugin>> {
        let url = &node.src;
        self.builders
            .iter()
            .find_map(|builder| builder.try_build(url))
            .ok_or_else(|| anyhow!("unsupported straw url: {url}"))
    }
}
