use anyhow::{anyhow, Result};
use ark_core_k8s::data::Url;

pub struct PluginBuilder<'a> {
    loaders: &'a [ModelLoader<'a>],
}

impl<'a> PluginBuilder<'a> {
    pub const fn new() -> Self {
        Self {
            loaders: &[ModelLoader {
                scheme: "huggingface",
                code: include_str!("./huggingface.py"),
            }],
        }
    }

    pub fn load_code(&self, model: &Url) -> Result<&'a str> {
        self.try_load(model)
            .map(|loader| loader.code)
            .ok_or_else(|| anyhow!("unsupported model URL scheme: {model}"))
    }

    fn try_load(&self, model: &Url) -> Option<&ModelLoader<'a>> {
        let scheme = model.scheme();
        self.loaders.iter().find(|&loader| loader.scheme == scheme)
    }
}

#[cfg(feature = "straw")]
impl ::straw_api::plugin::PluginBuilder for PluginBuilder<'static> {
    fn try_build(&self, url: &Url) -> Option<::straw_api::plugin::DynPlugin> {
        self.try_load(url)
            .map(|loader| Box::new(*loader) as ::straw_api::plugin::DynPlugin)
    }
}

#[derive(Copy, Clone)]
pub struct ModelLoader<'a> {
    scheme: &'a str,
    code: &'a str,
}

#[cfg(feature = "straw")]
impl<'a> ::straw_api::plugin::PluginDaemon for ModelLoader<'a> {}
