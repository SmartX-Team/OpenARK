use ark_core_k8s::data::Url;
use k8s_openapi::api::core::v1::EnvVar;

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

    fn container_command(&self, env: &[EnvVar]) -> Option<Vec<String>> {
        if parse_executable_command(env).is_some() {
            Some(vec!["/usr/bin/env".into()])
        } else {
            None
        }
    }

    fn container_command_args(&self, env: &[EnvVar]) -> Option<Vec<String>> {
        parse_executable_command(env).map(|command| vec!["sh".into(), "-c".into(), command])
    }
}

fn parse_executable_command(env: &[EnvVar]) -> Option<String> {
    env.iter()
        .find(|env| env.name == "_COMMAND")
        .and_then(|env| env.value.clone())
}
