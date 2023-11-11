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
            Some(Box::new(Plugin { url: url.clone() }))
        } else {
            None
        }
    }
}

pub struct Plugin {
    url: Url,
}

impl ::straw_api::plugin::PluginDaemon for Plugin {
    fn container_image(&self) -> String {
        self.url.to_string()["oci://".len()..].into()
    }
}
