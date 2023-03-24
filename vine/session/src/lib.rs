use std::{fs, path::PathBuf};

use dash_actor::client::{job::FunctionActorJobClient, SessionContextMetadata};
use ipis::{
    core::{
        anyhow::{bail, Error, Result},
        chrono::{DateTime, Duration, NaiveDateTime, Utc},
    },
    env,
    futures::TryFutureExt,
    log::info,
};
use serde::Serialize;
use vine_api::{
    k8s_openapi::{api::core::v1::Node, Resource},
    kube::{
        api::{Patch, PatchParams},
        Api, Client, ResourceExt,
    },
    serde_json::json,
    user_box_quota::UserBoxQuotaSpec,
    user_role::UserRoleSpec,
};

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
    const LABEL_BIND_BY_USER: &'static str = "vine.ulagbulag.io/bind.user";
    const LABEL_BIND_STATUS: &'static str = "vine.ulagbulag.io/bind";
    const LABEL_BIND_TIMESTAMP: &'static str = "vine.ulagbulag.io/bind.timestamp";

    const TEMPLATE_CLEANUP_FILENAME: &'static str = "user-session-cleanup.yaml.j2";
    const TEMPLATE_NAMESPACE_FILENAME: &'static str = "user-session-namespace.yaml.j2";
    const TEMPLATE_SESSION_FILENAME: &'static str = "user-session.yaml.j2";

    pub async fn create(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx: SessionContext = spec.into();

        self.label_user(ctx.spec.node, Some(ctx.spec.user_name))
            .and_then(|()| self.create_namespace(&ctx))
            .and_then(|()| self.delete_cleanup(&ctx))
            .and_then(|()| self.create_template(&ctx))
            .await
    }

    pub async fn delete(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx: SessionContext = spec.into();

        self.create_namespace(&ctx)
            .and_then(|()| self.delete_template(&ctx))
            .and_then(|()| self.create_cleanup(&ctx))
            .and_then(|()| self.label_user(ctx.spec.node, None))
            .await
    }

    pub async fn try_unbind(&self, node: &Node) -> Result<Option<String>> {
        if let Some(user_name) = self.get_user_name(node)? {
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
                return self
                    .delete(ctx.spec)
                    .map_ok(|()| Some(ctx.spec.user_name.to_string()))
                    .await;
            }
        }
        Ok(None)
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

    fn get_user_name<'a>(&self, node: &'a Node) -> Result<Option<&'a str>> {
        let name = node.name_any();

        let labels = node.labels();
        if labels.get(Self::LABEL_BIND_STATUS).map(AsRef::as_ref) != Some("true") {
            info!("skipping unbinding node [{name}]: not binded");
            return Ok(None);
        }

        let duration_session_start = Duration::seconds(5);
        match labels
            .get(Self::LABEL_BIND_TIMESTAMP)
            .and_then(|timestamp| {
                let timestamp: i64 = timestamp.parse().ok()?;
                let naive_date_time = NaiveDateTime::from_timestamp_millis(timestamp)?;
                Some(DateTime::<Utc>::from_utc(naive_date_time, Utc))
            }) {
            Some(timestamp) if Utc::now() - timestamp >= duration_session_start => {}
            Some(_) => {
                info!("skipping unbinding node: {name:?}: session is in starting (timeout: {duration_session_start})");
                return Ok(None);
            }
            None => {
                info!("skipping unbinding node: {name:?}: timestamp is missing");
                return Ok(None);
            }
        }

        let user_name = match labels.get(Self::LABEL_BIND_BY_USER) {
            Some(user_name) => user_name,
            None => {
                info!("skipping unbinding node: {name:?}: username is missing");
                return Ok(None);
            }
        };

        Ok(Some(user_name))
    }

    async fn label_user(&self, node: &Node, user_name: Option<&str>) -> Result<()> {
        let api = Api::<Node>::all(self.client.kube.clone());
        let name = node.name_any();
        let pp = PatchParams {
            field_manager: Some(self::consts::NAME.into()),
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
                    Self::LABEL_BIND_TIMESTAMP: user_name.map(|_| Utc::now().timestamp_millis().to_string()),
                },
            },
        }));
        api.patch(&name, &pp, &patch)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

pub type SessionContext<'a> = ::dash_actor::client::SessionContext<&'a SessionContextSpec<'a>>;

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
                namespace: format!("vine-session-{}", &spec.user_name),
            },
            spec,
        }
    }
}
