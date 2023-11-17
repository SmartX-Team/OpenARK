#![cfg(any(feature = "code", feature = "straw"))]

use ark_core_k8s::data::Url;

pub struct PluginBuilder<'a> {
    loaders: &'a [ModelLoader<'a>],
}

impl<'a> PluginBuilder<'a> {
    pub const fn new() -> Self {
        Self {
            loaders: &[ModelLoader {
                scheme: "huggingface",
                #[cfg(feature = "code")]
                code: include_str!("./huggingface.py"),
            }],
        }
    }

    #[cfg(feature = "code")]
    pub fn load_code(&self, model: &Url) -> ::anyhow::Result<&'a str> {
        self.try_load(model)
            .map(|loader| loader.code)
            .ok_or_else(|| ::anyhow::anyhow!("unsupported model URL scheme: {model}"))
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
    #[cfg(feature = "code")]
    code: &'a str,
}

#[cfg(feature = "straw")]
impl<'a> ::straw_api::plugin::PluginDaemon for ModelLoader<'a> {
    fn container_default_env(
        &self,
        node: &::straw_api::function::StrawNode,
    ) -> Vec<::k8s_openapi::api::core::v1::EnvVar> {
        use inflector::Inflector;
        use k8s_openapi::api::core::v1::EnvVar;

        vec![
            EnvVar {
                name: "PIPE_AI_MODEL".into(),
                value: Some(node.src.to_string()),
                value_from: None,
            },
            EnvVar {
                name: "PIPE_AI_MODEL_KIND".into(),
                value: Some(node.name.to_pascal_case()),
                value_from: None,
            },
        ]
    }

    fn container_command(
        &self,
        _env: &[::k8s_openapi::api::core::v1::EnvVar],
    ) -> Option<Vec<String>> {
        Some(vec!["dash-pipe-function-ai".into()])
    }

    fn container_resources(&self) -> Option<::k8s_openapi::api::core::v1::ResourceRequirements> {
        use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

        Some(::k8s_openapi::api::core::v1::ResourceRequirements {
            claims: None,
            requests: None,
            limits: Some(::maplit::btreemap! {
                // "cpu".into() => Quantity("1".into()),
                // "memory".into() => Quantity("500Mi".into()),
                "nvidia.com/gpu".into() => Quantity("1".into()),
            }),
        })
    }
}
