use std::future::Future;

use anyhow::{bail, Result};
use dash_api::task::{TaskActorJobSpec, TaskActorSourceSpec};
use dash_provider_api::job::{TaskActorJobMetadata, TaskChannelKindJob, TemplateRef};
use kube::{
    api::{DeleteParams, Patch, PatchParams, PostParams},
    core::DynamicObject,
    discovery::{self, Scope},
    Api, Client,
};
use serde::Serialize;
use tera::{Context, Tera};
use tracing::{instrument, Level};

use crate::storage::KubernetesStorageClient;

use super::SessionContext;

pub struct TaskActorJobClient {
    pub kube: Client,
    metadata: TaskActorJobMetadata,
    name: String,
    namespace: String,
    tera: Tera,
    use_prefix: bool,
}

impl TaskActorJobClient {
    #[instrument(level = Level::INFO, skip(kube, spec), err(Display))]
    pub async fn try_new(
        namespace: String,
        kube: &Client,
        spec: &TaskActorJobSpec,
        use_prefix: bool,
    ) -> Result<Self> {
        let client = KubernetesStorageClient {
            namespace: &namespace,
            kube,
        };
        let (name, content) = match &spec.source {
            TaskActorSourceSpec::ConfigMapRef(spec) => client.load_config_map(spec).await?,
        };

        Self::from_raw_content(
            kube.clone(),
            spec.metadata.clone(),
            namespace,
            name,
            &content,
            use_prefix,
        )
    }

    pub fn from_dir(
        metadata: TaskActorJobMetadata,
        namespace: String,
        kube: Client,
        path: &str,
        use_prefix: bool,
    ) -> Result<Self> {
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
            metadata,
            name: Default::default(),
            namespace,
            tera,
            use_prefix,
        })
    }

    pub fn from_raw_content(
        kube: Client,
        metadata: TaskActorJobMetadata,
        namespace: String,
        name: &str,
        content: &str,
        use_prefix: bool,
    ) -> Result<Self> {
        let mut tera = Tera::default();
        tera.add_raw_template(name, content)?;

        Ok(Self {
            kube,
            metadata,
            name: name.to_string(),
            namespace,
            tera,
            use_prefix,
        })
    }
}

impl TaskActorJobClient {
    pub const fn kube(&self) -> &Client {
        &self.kube
    }

    pub fn namespace(&self) -> &str {
        self.namespace.as_str()
    }

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    pub async fn exists<Spec>(&self, input: &SessionContext<Spec>) -> Result<bool>
    where
        Spec: Serialize,
    {
        self.exists_named(&self.name, input).await
    }

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    pub async fn exists_named<Spec>(&self, name: &str, input: &SessionContext<Spec>) -> Result<bool>
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

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    pub async fn create<Spec>(&self, input: &SessionContext<Spec>) -> Result<TaskChannelKindJob>
    where
        Spec: Serialize,
    {
        self.create_named(&self.name, input).await
    }

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    pub async fn create_named<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<TaskChannelKindJob>
    where
        Spec: Serialize,
    {
        self.execute_with(name, input, try_create).await
    }

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    pub async fn delete<Spec>(&self, input: &SessionContext<Spec>) -> Result<TaskChannelKindJob>
    where
        Spec: Serialize,
    {
        self.delete_named(&self.name, input).await
    }

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    pub async fn delete_named<Spec>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
    ) -> Result<TaskChannelKindJob>
    where
        Spec: Serialize,
    {
        self.execute_with(name, input, try_delete).await
    }

    #[instrument(level = Level::INFO, skip(self, input, f), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
    async fn execute_with<Spec, F, Fut>(
        &self,
        name: &str,
        input: &SessionContext<Spec>,
        f: F,
    ) -> Result<TaskChannelKindJob>
    where
        Spec: Serialize,
        F: Fn(Template, bool) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let templates = self.load_template(name, input).await?;
        let result = TaskChannelKindJob {
            metadata: self.metadata.clone(),
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

    #[instrument(level = Level::INFO, skip(self, input), fields(metadata.name = %input.metadata.name, metadata.namespace = %input.metadata.namespace), err(Display))]
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
        let templates: Vec<DynamicObject> = ::serde_yaml::Deserializer::from_str(&templates)
            .map(::serde::Deserialize::deserialize)
            .collect::<Result<_, _>>()?;

        // create templates

        let mut apis = vec![];
        for mut template in templates {
            let name = {
                let prefix = &input.metadata.name;
                let name = template.metadata.name.get_or_insert_with(|| prefix.clone());

                if self.use_prefix && !name.starts_with(prefix) {
                    bail!("name should be started with {prefix:?}: {name:?}",)
                }
                name
            };

            let namespace = {
                let prefix = &input.metadata.namespace;
                let namespace = template
                    .metadata
                    .namespace
                    .get_or_insert_with(|| prefix.clone());

                if self.use_prefix && !namespace.starts_with(prefix) {
                    bail!("namespace should be started with {prefix:?}: {namespace:?}",)
                }
                namespace
            };

            let types = match template.types.as_ref() {
                Some(types) => types,
                None => bail!("untyped document is not supported: {name:?}"),
            };

            let (api_group, version) = {
                let mut iter = types.api_version.split('/');
                match (iter.next(), iter.next()) {
                    (Some(api_group), Some(version)) => (api_group, Some(version)),
                    (Some(_), None) | (None, _) => ("", None),
                }
            };

            // Discover most stable version variant of document
            let apigroup = discovery::group(&self.kube, api_group).await?;
            let (ar, caps) = match match version {
                Some(version) => apigroup.versioned_resources(version),
                None => apigroup.recommended_resources(),
            }
            .into_iter()
            .find(|(ar, _)| ar.kind == types.kind)
            {
                Some((ar, caps)) => (ar, caps),
                None => bail!(
                    "Cannot find CRD: {kind}.{api_version}",
                    api_version = types.api_version,
                    kind = types.kind,
                ),
            };

            // Detect the immutable resources
            let immutable = matches!(
                (ar.group.as_str(), ar.kind.as_str()),
                ("", "PersistentVolume") | ("storage.k8s.io", "StorageClass")
            );

            // Use the discovered kind in an Api, and Controller with the ApiResource as its DynamicType
            let api: Api<DynamicObject> = match caps.scope {
                Scope::Cluster => Api::all_with(self.kube.clone(), &ar),
                Scope::Namespaced => Api::namespaced_with(self.kube.clone(), namespace, &ar),
            };
            apis.push(Template {
                api,
                immutable,
                name: name.clone(),
                template,
            });
        }
        Ok(apis)
    }
}

#[derive(Debug)]
struct Template {
    api: Api<DynamicObject>,
    immutable: bool,
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

#[instrument(level = Level::INFO, skip(template), fields(template.name = %template.name), err(Display))]
async fn try_create(template: Template, exists: bool) -> Result<()> {
    if exists {
        // Skip applying to immutable resources
        if template.immutable {
            return Ok(());
        }

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

#[instrument(level = Level::INFO, skip(template), fields(template.name = %template.name), err(Display))]
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
