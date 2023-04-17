use std::{collections::BTreeMap, path::Path};

use ark_actor_api::package::Package;
use ark_api::package::ArkPackageCrd;
use ipis::{core::anyhow::Result, tokio::fs};
use serde::Serialize;
use tera::{Context, Tera};

pub struct TemplateManager {
    default_context: DefaultContext<'static>,
    tera: Tera,
}

impl TemplateManager {
    const TEMPLATE_NAME: &'static str = "Containerfile.j2";

    pub async fn try_from_local(path: &Path) -> Result<Self> {
        Ok(Self {
            default_context: Default::default(),
            tera: {
                let content = fs::read_to_string(path).await?;

                let mut tera = Tera::default();
                tera.add_raw_template(Self::TEMPLATE_NAME, &content)?;
                tera
            },
        })
    }

    pub fn render_build(&self, Package { name, resource }: &Package) -> Result<Template> {
        let context = Context::from_serialize(&BuildContext {
            default: &self.default_context,
            resource,
        })?;

        self.tera
            .render(Self::TEMPLATE_NAME, &context)
            .map(|text| Template {
                name: name.clone(),
                text,
                version: resource.get_image_version().to_string(),
            })
            .map_err(Into::into)
    }
}

pub struct Template {
    pub name: String,
    pub text: String,
    pub version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildContext<'a> {
    default: &'a DefaultContext<'static>,
    #[serde(flatten)]
    resource: &'a ArkPackageCrd,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultContext<'a> {
    os_image_list: BTreeMap<&'a str, &'a str>,
}

impl Default for DefaultContext<'static> {
    fn default() -> Self {
        Self {
            os_image_list: [
                ("alpine", "docker.io/library/alpine"),
                ("archlinux", "docker.io/library/archlinux"),
                ("rockylinux", "quay.io/rockylinux/rockylinux"),
            ]
            .iter()
            .copied()
            .collect(),
        }
    }
}
