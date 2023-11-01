use anyhow::{anyhow, Result};
use ark_core_k8s::data::Url;

pub struct Plugin<'a> {
    loaders: &'a [ModelLoader<'a>],
}

impl<'a> Plugin<'a> {
    pub const fn new() -> Self {
        Self {
            loaders: &[ModelLoader {
                scheme: "huggingface",
                code: include_str!("./huggingface.py"),
            }],
        }
    }

    pub fn load_code(&self, model: &Url) -> Result<&'a str> {
        self.loaders
            .iter()
            .find(|&loader| loader.scheme == model.scheme())
            .map(|loader| loader.code)
            .ok_or_else(|| anyhow!("unsupported model URL scheme: {model}"))
    }
}

#[cfg(feature = "straw")]
impl ::straw_api::plugin::PluginBuilder for Plugin<'static> {
    fn try_build(
        &self,
        scheme: &str,
    ) -> Result<Option<Box<dyn ::straw_api::plugin::Plugin>>> {
        Ok(self
            .loaders
            .iter()
            .find(|&loader| loader.scheme == scheme)
            .map(|loader| Box::new(*loader) as Box<dyn ::straw_api::plugin::Plugin>))
    }
}

#[derive(Copy, Clone)]
pub struct ModelLoader<'a> {
    scheme: &'a str,
    code: &'a str,
}

#[cfg(feature = "straw")]
#[::async_trait::async_trait]
impl<'a> ::straw_api::plugin::Plugin for ModelLoader<'a> {
    async fn create(&self, client: &::kube::Client) -> Result<()> {
        todo!()
    }

    async fn delete(&self, client: &::kube::Client) -> Result<()> {
        todo!()
    }
}
