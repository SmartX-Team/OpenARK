use std::{future::Future, path::PathBuf};

use ipis::{
    async_trait::async_trait,
    core::anyhow::{bail, Error, Result},
    env::{self, Infer},
    tokio::fs,
};
use serde::Serialize;
use tera::{Context, Tera};
use vine_api::{
    k8s_openapi::api::core::v1::Node,
    kube::{
        api::{DeleteParams, Patch, PatchParams, PostParams},
        core::DynamicObject,
        discovery, Api, Client, ResourceExt,
    },
    user::UserCrd,
};

pub struct SessionManager {
    tera: Tera,
}

#[async_trait]
impl<'a> Infer<'a> for SessionManager {
    type GenesisArgs = ();
    type GenesisResult = Self;

    async fn try_infer() -> Result<Self>
    where
        Self: Sized,
    {
        <Self as Infer<'a>>::genesis(()).await
    }

    async fn genesis(
        (): <Self as Infer<'a>>::GenesisArgs,
    ) -> Result<<Self as Infer<'a>>::GenesisResult> {
        let templates_home = env::infer("VINE_SESSION_TEMPLATES_HOME").or_else(|_| {
            // local directory
            "../../templates/vine/templates/session"
                .parse::<PathBuf>()
                .map_err(Error::from)
        })?;
        let templates_home = fs::canonicalize(templates_home).await?;

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
    pub const TEMPLATE_FILENAME: &'static str = "user-session.yaml.j2";

    pub async fn create(&self, kube: &Client, node: &Node, user: &UserCrd) -> Result<()> {
        async fn execute(
            api: Api<DynamicObject>,
            template: DynamicObject,
            exists: bool,
        ) -> Result<()> {
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

        self.execute(kube, node, user, execute).await
    }

    pub async fn delete(&self, kube: &Client, node: &Node, user: &UserCrd) -> Result<()> {
        async fn execute(
            api: Api<DynamicObject>,
            template: DynamicObject,
            exists: bool,
        ) -> Result<()> {
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

        self.execute(kube, node, user, execute).await
    }

    async fn execute<F, Fut>(&self, kube: &Client, node: &Node, user: &UserCrd, f: F) -> Result<()>
    where
        F: Fn(Api<DynamicObject>, DynamicObject, bool) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let context = Context::from_serialize(&SessionContext { node, user })?;
        let templates = self.tera.render(Self::TEMPLATE_FILENAME, &context)?;
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionContext<'a> {
    node: &'a Node,
    user: &'a UserCrd,
}
