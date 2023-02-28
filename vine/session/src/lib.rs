use std::{fs, future::Future, path::PathBuf};

use ipis::{
    core::anyhow::{bail, Error, Result},
    env,
    futures::TryFutureExt,
    log::info,
};
use serde::Serialize;
use tera::{Context, Tera};
use vine_api::{
    k8s_openapi::{api::core::v1::Node, Resource},
    kube::{
        api::{DeleteParams, Patch, PatchParams, PostParams},
        core::DynamicObject,
        discovery, Api, Client, ResourceExt,
    },
    serde_json::json,
};

pub struct SessionManager {
    tera: Tera,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::try_default().expect("failed to create SessionManager")
    }
}

impl SessionManager {
    pub fn try_default() -> Result<Self> {
        let templates_home = env::infer("VINE_SESSION_TEMPLATES_HOME").or_else(|_| {
            // local directory
            "../../templates/vine/templates/session"
                .parse::<PathBuf>()
                .map_err(Error::from)
        })?;
        let templates_home = fs::canonicalize(templates_home)?;

        let mut tera = match templates_home.to_str() {
            Some(templates_home) => match Tera::new(&format!("{templates_home}/*.yaml.j2")) {
                Ok(tera) => tera,
                Err(e) => {
                    println!("Parsing error(s): {}", e);
                    ::std::process::exit(1);
                }
            },
            None => bail!("failed to parse the environment variable: VINE_SESSION_TEMPLATES_HOME = {templates_home:?}"),
        };
        tera.autoescape_on(vec![".yaml.j2"]);

        Ok(Self { tera })
    }
}

impl SessionManager {
    pub const NAME: &'static str = "vine-session";

    const LABEL_BIND_BY_USER: &'static str = "vine.ulagbulag.io/bind.user";
    const LABEL_BIND_STATUS: &'static str = "vine.ulagbulag.io/bind";

    const TEMPLATE_CLEANUP_FILENAME: &'static str = "user-session-cleanup.yaml.j2";
    const TEMPLATE_SESSION_FILENAME: &'static str = "user-session.yaml.j2";

    pub async fn create(&self, kube: &Client, node: &Node, user_name: &str) -> Result<()> {
        self.label_user(kube, node, Some(user_name))
            .and_then(|()| self.cleanup(kube, node, user_name, try_delete))
            .and_then(|()| self.execute(kube, node, user_name, try_create))
            .await
    }

    pub async fn delete(&self, kube: &Client, node: &Node, user_name: &str) -> Result<()> {
        self.execute(kube, node, user_name, try_delete)
            .and_then(|()| self.cleanup(kube, node, user_name, try_create))
            .and_then(|()| self.label_user(kube, node, None))
            .await
    }

    async fn cleanup<F, Fut>(&self, kube: &Client, node: &Node, user_name: &str, f: F) -> Result<()>
    where
        F: Fn(Api<DynamicObject>, DynamicObject, bool) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        self.execute_with(kube, node, user_name, Self::TEMPLATE_CLEANUP_FILENAME, f)
            .await
    }

    pub async fn unbind(&self, kube: &Client, node: &Node) -> Result<Option<String>> {
        let name = node.name_any();

        let labels = node.labels();
        if labels.get(Self::LABEL_BIND_STATUS).map(AsRef::as_ref) != Some("true") {
            info!("skipping unbinding node [{name}]: not binded");
            return Ok(None);
        }

        let user_name = match labels.get(Self::LABEL_BIND_BY_USER) {
            Some(user_name) => user_name,
            None => {
                info!("skipping unbinding node: {name:?}: username is missing");
                return Ok(None);
            }
        };

        self.delete(kube, node, user_name)
            .map_ok(|()| Some(user_name.clone()))
            .await
    }

    async fn label_user(&self, kube: &Client, node: &Node, user_name: Option<&str>) -> Result<()> {
        let api = Api::<Node>::all(kube.clone());
        let name = node.name_any();
        let pp = PatchParams {
            field_manager: Some(SessionManager::NAME.into()),
            force: true,
            ..Default::default()
        };

        let patch = Patch::Apply(json!({
            "apiVersion": Node::API_VERSION,
            "kind": Node::KIND,
            "metadata": {
                "labels": {
                    Self::LABEL_BIND_BY_USER: user_name,
                    Self::LABEL_BIND_STATUS: user_name.is_some().to_string(),
                },
            },
        }));
        api.patch(&name, &pp, &patch)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn execute<F, Fut>(&self, kube: &Client, node: &Node, user_name: &str, f: F) -> Result<()>
    where
        F: Fn(Api<DynamicObject>, DynamicObject, bool) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        self.execute_with(kube, node, user_name, Self::TEMPLATE_SESSION_FILENAME, f)
            .await
    }

    async fn execute_with<F, Fut>(
        &self,
        kube: &Client,
        node: &Node,
        user_name: &str,
        template_name: &str,
        f: F,
    ) -> Result<()>
    where
        F: Fn(Api<DynamicObject>, DynamicObject, bool) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let context = Context::from_serialize(&SessionContext { node, user_name })?;
        let templates = self.tera.render(template_name, &context)?;
        let templates: Vec<DynamicObject> = ::serde_yaml::Deserializer::from_str(&templates)
            .map(::serde::Deserialize::deserialize)
            .collect::<Result<Vec<_>, _>>()?;

        // create user session namespace

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
            let apigroup = discovery::group(kube, api_group).await?;
            let (ar, _caps) = apigroup.recommended_kind(&types.kind).unwrap();

            // Use the discovered kind in an Api, and Controller with the ApiResource as its DynamicType
            let api: Api<DynamicObject> = match &namespace {
                Some(namespace) => Api::namespaced_with(kube.clone(), namespace, &ar),
                None => Api::all_with(kube.clone(), &ar),
            };

            // Update documents
            match api.get_opt(&name).await? {
                Some(_) => f(api, template, true).await?,
                None => f(api, template, false).await?,
            }
        }
        Ok(())
    }
}

async fn try_create(api: Api<DynamicObject>, template: DynamicObject, exists: bool) -> Result<()> {
    if exists {
        let pp = PatchParams {
            field_manager: Some(SessionManager::NAME.into()),
            force: true,
            ..Default::default()
        };

        api.patch(&template.name_any(), &pp, &Patch::Apply(template))
            .await
            .map(|_| ())
            .map_err(Into::into)
    } else {
        let pp = PostParams {
            field_manager: Some(SessionManager::NAME.into()),
            ..Default::default()
        };

        api.create(&pp, &template)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

async fn try_delete(api: Api<DynamicObject>, template: DynamicObject, exists: bool) -> Result<()> {
    if exists {
        let dp = DeleteParams::default();

        api.delete(&template.name_any(), &dp)
            .await
            .map(|_| ())
            .map_err(Into::into)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionContext<'a> {
    node: &'a Node,
    user_name: &'a str,
}
