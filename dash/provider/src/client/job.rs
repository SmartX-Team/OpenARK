use std::future::Future;

use dash_api::{
    function::FunctionActorJobSpec,
    kube::{
        api::{DeleteParams, Patch, PatchParams, PostParams},
        core::DynamicObject,
        discovery, Api, Client, ResourceExt,
    },
    serde_yaml,
};
use ipis::core::anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

use crate::storage::KubernetesStorageClient;

use super::SessionContext;

pub struct FunctionActorJobClient {
    pub kube: Client,
    name: String,
    tera: Tera,
}

impl FunctionActorJobClient {
    pub async fn try_new(kube: &Client, spec: &FunctionActorJobSpec) -> Result<Self> {
        let client = KubernetesStorageClient { kube };
        let (name, content) = match spec {
            FunctionActorJobSpec::ConfigMapRef(spec) => client.load_config_map(spec).await?,
        };

        Self::from_raw_content(kube.clone(), name, &content)
    }

    pub fn from_dir(kube: Client, path: &str) -> Result<Self> {
        let mut tera = match Tera::new(path) {
            Ok(tera) => tera,
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        tera.autoescape_on(vec![".yaml.j2"]);

        Ok(Self {
            kube,
            name: Default::default(),
            tera,
        })
    }

    fn from_raw_content(kube: Client, name: &str, content: &str) -> Result<Self> {
        let mut tera = Tera::default();
        tera.add_raw_template(name, content)?;

        Ok(Self {
            kube,
            name: name.to_string(),
            tera,
        })
    }
}

impl FunctionActorJobClient {
    pub const fn kube(&self) -> &Client {
        &self.kube
    }

    pub async fn exists_raw<Spec>(&self, input: &SessionContext<Spec>) -> Result<bool>
    where
        Spec: Serialize,
    {
        self.exists_raw_named(&self.name, input).await
    }

    pub async fn exists_raw_named<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<bool>
    where
        Spec: Serialize,
    {
        self.execute_raw_any_with(name, input).await
    }

    pub async fn create_raw<Spec>(
        &self,
        input: &SessionContext<Spec>,
    ) -> Result<FunctionChannelKindJob>
    where
        Spec: Serialize,
    {
        self.create_raw_named(&self.name, input).await
    }

    pub async fn create_raw_named<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<FunctionChannelKindJob>
    where
        Spec: Serialize,
    {
        self.execute_raw_with(name, input, try_create).await
    }

    pub async fn delete_raw<Spec>(
        &self,
        input: &SessionContext<Spec>,
    ) -> Result<FunctionChannelKindJob>
    where
        Spec: Serialize,
    {
        self.delete_raw_named(&self.name, input).await
    }

    pub async fn delete_raw_named<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<FunctionChannelKindJob>
    where
        Spec: Serialize,
    {
        self.execute_raw_with(name, input, try_delete).await
    }

    async fn execute_raw_with<Spec, F, Fut>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
        f: F,
    ) -> Result<FunctionChannelKindJob>
    where
        Spec: Serialize,
        F: Fn(Template, bool) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let templates = self.load_template(name, input).await?;
        let result = FunctionChannelKindJob {
            templates: templates.iter().map(Into::into).collect(),
        };

        for template in templates {
            // Update documents
            match template.api.get_opt(&template.name).await? {
                Some(_) => f(template, true).await?,
                None => f(template, false).await?,
            }
        }
        Ok(result)
    }

    async fn execute_raw_any_with<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<bool>
    where
        Spec: Serialize,
    {
        for template in self.load_template(name, input).await? {
            // Find documents
            if template.api.get_opt(&template.name).await?.is_none() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn load_template<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<Vec<Template>>
    where
        Spec: Serialize,
    {
        let context = Context::from_serialize(input)?;
        let templates = self.tera.render(name, &context)?;
        let templates: Vec<DynamicObject> = serde_yaml::Deserializer::from_str(&templates)
            .map(::serde::Deserialize::deserialize)
            .collect::<Result<_, _>>()?;

        // create templates

        let mut apis = vec![];
        for template in templates {
            let name = template.name_any();
            let namespace = template.namespace();
            let types = match template.types.as_ref() {
                Some(types) => types,
                None => bail!("untyped document is not supported: {name}"),
            };

            let api_group = {
                let mut iter = types.api_version.split('/');
                match (iter.next(), iter.next()) {
                    (Some(api_group), Some(_)) => api_group,
                    (Some(_), None) | (None, _) => "",
                }
            };

            // Discover most stable version variant of document
            let apigroup = discovery::group(&self.kube, api_group).await?;
            let (ar, _caps) = apigroup.recommended_kind(&types.kind).unwrap();

            // Use the discovered kind in an Api, and Controller with the ApiResource as its DynamicType
            let api: Api<DynamicObject> = match &namespace {
                Some(namespace) => Api::namespaced_with(self.kube.clone(), namespace, &ar),
                None => Api::all_with(self.kube.clone(), &ar),
            };
            apis.push(Template {
                api,
                name,
                template,
            });
        }
        Ok(apis)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionChannelKindJob {
    pub templates: Vec<TemplateRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateRef {
    pub name: String,
}

struct Template {
    api: Api<DynamicObject>,
    name: String,
    template: DynamicObject,
}

impl From<&Template> for TemplateRef {
    fn from(value: &Template) -> Self {
        Self {
            name: value.name.clone(),
        }
    }
}

async fn try_create(template: Template, exists: bool) -> Result<()> {
    if exists {
        let pp = PatchParams {
            field_manager: Some(crate::NAME.into()),
            force: true,
            ..Default::default()
        };

        template
            .api
            .patch(&template.name, &pp, &Patch::Apply(&template.template))
            .await
            .map(|_| ())
            .map_err(Into::into)
    } else {
        let pp = PostParams {
            field_manager: Some(crate::NAME.into()),
            ..Default::default()
        };

        template
            .api
            .create(&pp, &template.template)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

async fn try_delete(template: Template, exists: bool) -> Result<()> {
    // skip deleting PersistentVolumeClaim
    if let Some(types) = &template.template.types {
        if types.api_version == "v1" && types.kind == "PersistentVolumeClaim" {
            return Ok(());
        }
    }

    if exists {
        let dp = DeleteParams::default();

        template
            .api
            .delete(&template.name, &dp)
            .await
            .map(|_| ())
            .map_err(Into::into)
    } else {
        Ok(())
    }
}
