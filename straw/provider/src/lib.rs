use anyhow::{anyhow, Result};
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::Client;
use straw_api::{
    pipe::{StrawNode, StrawPipe},
    plugin::{Plugin, PluginBuilder},
};

pub struct StrawSession {
    builders: Vec<Box<dyn PluginBuilder>>,
    kube: Client,
}

impl StrawSession {
    pub fn new(kube: Client) -> Self {
        Self {
            builders: vec![
                #[cfg(feature = "ai")]
                Box::new(::dash_pipe_function_ai::plugin::Plugin::new()),
            ],
            kube,
        }
    }

    pub fn add_plugin(&mut self, builder: impl Into<Box<dyn PluginBuilder>>) {
        self.builders.push(builder.into())
    }

    pub async fn create(&self, pipe: &StrawPipe) -> Result<()> {
        pipe.straw
            .iter()
            .map(|node| self.create_node(node))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }

    async fn create_node(&self, node: &StrawNode) -> Result<()> {
        self.load_plugin(node).await?.create(&self.kube).await
    }

    async fn load_plugin(&self, node: &StrawNode) -> Result<Box<dyn Plugin>> {
        let scheme = node.src.scheme();

        self.builders
            .iter()
            .find_map(|builder| builder.try_build(scheme).transpose())
            .transpose()?
            .ok_or_else(|| anyhow!("unsupported straw url: {url}", url = &node.src,))
    }
}
