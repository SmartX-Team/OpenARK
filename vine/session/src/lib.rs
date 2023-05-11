use std::{fmt, fs, path::PathBuf};

use anyhow::{bail, Error, Result};
use ark_api::{NamespaceAny, SessionRef};
use ark_core::env;
use chrono::Utc;
use dash_provider::client::job::FunctionActorJobClient;
use dash_provider_api::SessionContextMetadata;
use futures::TryFutureExt;
use k8s_openapi::{api::core::v1::Node, serde_json::Value, Metadata, Resource};
use kube::{
    api::{Patch, PatchParams},
    core::ObjectMeta,
    Api, Client, ResourceExt,
};
use log::info;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use vine_api::{user_box_quota::UserBoxQuotaSpec, user_role::UserRoleSpec};

pub(crate) mod consts {
    pub const NAME: &str = "vine-session";
}

pub struct SessionManager {
    client: FunctionActorJobClient,
}

impl SessionManager {
    pub async fn try_new(kube: Client) -> Result<Self> {
        let templates_home = env::infer("VINE_SESSION_TEMPLATES_HOME").or_else(|_| {
            // local directory
            "../../templates/vine/templates/session"
                .parse::<PathBuf>()
                .map_err(Error::from)
        })?;
        let templates_home = fs::canonicalize(templates_home)?;

        match templates_home.to_str() {
            Some(templates_home) => {
                let templates_home = format!("{templates_home}/*.yaml.j2");
                let client = FunctionActorJobClient::from_dir(kube, &templates_home)?;
                Ok(Self { client })
            },
            None => bail!("failed to parse the environment variable: VINE_SESSION_TEMPLATES_HOME = {templates_home:?}"),
        }
    }
}

impl SessionManager {
    const TEMPLATE_CLEANUP_FILENAME: &'static str = "user-session-cleanup.yaml.j2";
    const TEMPLATE_NAMESPACE_FILENAME: &'static str = "user-session-namespace.yaml.j2";
    const TEMPLATE_SESSION_FILENAME: &'static str = "user-session.yaml.j2";

    pub async fn create(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx: SessionContext = spec.into();

        self.label_node(ctx.spec.node, Some(ctx.spec.user_name))
            .and_then(|()| self.label_namespace(&ctx, Some(ctx.spec.user_name)))
            .and_then(|()| self.delete_cleanup(&ctx))
            .and_then(|()| self.create_template(&ctx))
            .await
    }

    pub async fn delete(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx: SessionContext = spec.into();

        self.label_namespace(&ctx, None)
            .and_then(|()| self.delete_template(&ctx))
            .and_then(|()| self.create_cleanup(&ctx))
            .and_then(|()| self.label_node(ctx.spec.node, None))
            .await
    }

    pub async fn try_unbind(&self, node: &Node) -> Result<Option<String>> {
        match node.get_session_ref() {
            Ok(SessionRef { user_name, .. }) => {
                let spec = SessionContextSpec {
                    box_quota: None,
                    node,
                    role: None,
                    user_name,
                };
                let ctx: SessionContext = (&spec).into();

                if
                // If the node is not ready
                !node
                .status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .and_then(|conditions| {
                    conditions
                        .iter()
                        .find(|condition| condition.type_ == "Ready")
                })
                .map(|condition| condition.status == "True")
                .unwrap_or(false)
                ||
                // If the node's managed session has been logged out
                !self.exists_template(&ctx).await?
                {
                    self.delete(ctx.spec)
                        .map_ok(|()| Some(ctx.spec.user_name.to_string()))
                        .await
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                info!("skipping unbinding node: {e}");
                Ok(None)
            }
        }
    }

    async fn exists_template(&self, ctx: &SessionContext<'_>) -> Result<bool> {
        self.client
            .exists_raw_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
    }

    async fn create_namespace(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_raw_named(Self::TEMPLATE_NAMESPACE_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn create_template(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_raw_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn delete_template(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .delete_raw_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn create_cleanup(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_raw_named(Self::TEMPLATE_CLEANUP_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn delete_cleanup(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .delete_raw_named(Self::TEMPLATE_CLEANUP_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn label_namespace(
        &self,
        ctx: &SessionContext<'_>,
        user_name: Option<&str>,
    ) -> Result<()> {
        self.create_namespace(ctx).await?;

        let name = ctx.spec.namespace();
        self.label::<Node>(&name, ctx.spec.node, user_name).await
    }

    async fn label_node(&self, node: &Node, user_name: Option<&str>) -> Result<()> {
        let name = node.name_any();
        self.label::<Node>(&name, node, user_name).await
    }

    async fn label<K>(&self, name: &str, node: &Node, user_name: Option<&str>) -> Result<()>
    where
        K: Clone + fmt::Debug + DeserializeOwned + Metadata<Ty = ObjectMeta>,
    {
        let api = Api::<K>::all(self.client.kube.clone());
        let pp = PatchParams {
            field_manager: Some(self::consts::NAME.into()),
            force: true,
            ..Default::default()
        };

        let node_name = node.name_any();
        let patch = Patch::Apply(json!({
            "apiVersion": Node::API_VERSION,
            "kind": Node::KIND,
            "metadata": {
                "name": name,
                "labels": get_label(&node_name, user_name),
            },
        }));
        api.patch(name, &pp, &patch)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

pub type SessionContext<'a> = ::dash_provider_api::SessionContext<&'a SessionContextSpec<'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextSpec<'a> {
    pub box_quota: Option<&'a UserBoxQuotaSpec>,
    pub node: &'a Node,
    pub role: Option<&'a UserRoleSpec>,
    pub user_name: &'a str,
}

impl<'a> From<&'a SessionContextSpec<'a>> for SessionContext<'a> {
    fn from(spec: &'a SessionContextSpec<'a>) -> Self {
        SessionContext {
            metadata: SessionContextMetadata {
                name: "".to_string(), // not used
                namespace: spec.namespace(),
            },
            spec,
        }
    }
}

impl<'a> SessionContextSpec<'a> {
    fn namespace(&self) -> String {
        format!("vine-session-{}", &self.user_name)
    }
}

fn get_label(node_name: &str, user_name: Option<&str>) -> Value {
    json!({
        ::ark_api::consts::LABEL_BIND_BY_USER: user_name,
        ::ark_api::consts::LABEL_BIND_NODE: node_name,
        ::ark_api::consts::LABEL_BIND_STATUS: user_name.is_some().to_string(),
        ::ark_api::consts::LABEL_BIND_TIMESTAMP: user_name.map(|_| Utc::now().timestamp_millis().to_string()),
    })
}
