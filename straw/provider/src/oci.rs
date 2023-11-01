use ark_core_k8s::data::Url;

pub struct PluginBuilder;

impl PluginBuilder {
    pub const fn new() -> Self {
        Self
    }
}

impl ::straw_api::plugin::PluginBuilder for PluginBuilder {
    fn try_build(&self, url: &Url) -> Option<::straw_api::plugin::DynPlugin> {
        if url.scheme() == "oci" {
            Some(Box::new(Plugin))
        } else {
            None
        }
    }
}

pub struct Plugin;

impl ::straw_api::plugin::PluginDaemon for Plugin {}
