#[cfg(feature = "batch")]
pub mod batch;
#[cfg(feature = "exec")]
pub mod exec;
#[cfg(feature = "shell")]
pub mod shell;

use std::{collections::BTreeMap, fmt, fs, path::PathBuf, time::Duration};

use anyhow::{bail, Error, Result};
use ark_api::{NamespaceAny, SessionRef};
use ark_core::env;
use chrono::Utc;
use dash_provider::client::job::TaskActorJobClient;
use dash_provider_api::SessionContextMetadata;
use futures::TryFutureExt;
use k8s_openapi::{
    api::core::v1::{Namespace, Node, Pod},
    serde_json::Value,
};
use kiss_api::r#box::BoxCrd;
use kube::{
    api::{DeleteParams, ListParams, Patch, PatchParams},
    Api, Client, Resource, ResourceExt,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tracing::{info, instrument, Level};
use vine_api::{user::UserCrd, user_box_quota::UserBoxQuotaSpec, user_role::UserRoleSpec};

pub(crate) mod consts {
    pub const NAME: &str = "vine-session";
}

pub struct SessionManager {
    client: TaskActorJobClient,
}

impl SessionManager {
    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    pub async fn try_new(namespace: String, kube: Client) -> Result<Self> {
        let templates_home = env::infer("VINE_SESSION_TEMPLATES_HOME").or_else(|_| {
            // local directory
            "../../../templates/vine/templates/session"
                .parse::<PathBuf>()
                .map_err(Error::from)
        })?;
        let templates_home = fs::canonicalize(templates_home)?;

        match templates_home.to_str() {
            Some(templates_home) => {
                let metadata = Default::default();
                let templates_home = format!("{templates_home}/*.yaml.j2");
                let use_prefix = false;
                let client = TaskActorJobClient::from_dir(metadata, namespace, kube, &templates_home, use_prefix)?;
                Ok(Self { client })
            },
            None => bail!("failed to parse the environment variable: VINE_SESSION_TEMPLATES_HOME = {templates_home:?}"),
        }
    }
}

impl SessionManager {
    const TEMPLATE_NAMESPACE_FILENAME: &'static str = "user-session-namespace.yaml.j2";
    const TEMPLATE_SESSION_FILENAME: &'static str = "user-session.yaml.j2";

    const THRESHOLD_SESSION_TIMEOUT: Duration = Duration::from_secs(30 * 60); // 30 minutes

    #[instrument(level = Level::INFO, skip(self, spec), fields(node_name = %spec.node.name_any(), user_name = %spec.user_name), err(Display))]
    pub async fn try_create(
        &self,
        spec: &SessionContextSpec<'_>,
        delete_on_fail: bool,
    ) -> Result<()> {
        match self.create(spec).await {
            Ok(()) => Ok(()),
            Err(error_create) => {
                if delete_on_fail {
                    match self.delete(spec).await {
                        Ok(()) => Err(error_create),
                        Err(error_revert) => bail!("{error_create}\n{error_revert}"),
                    }
                } else {
                    Err(error_create)
                }
            }
        }
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node_name = %node.name_any()), err(Display))]
    pub async fn try_delete(&self, node: &Node) -> Result<Option<String>> {
        match node
            .get_session_ref()
            .and_then(|session| session.assert_started().map(|()| session))
        {
            Ok(SessionRef { user_name, .. }) => {
                let spec = SessionContextSpec {
                    box_quota: None,
                    node,
                    persistence: is_persistent(node),
                    role: None,
                    user_name: &user_name,
                };
                let ctx = self.get_context(&spec);

                if
                // If the node is not ready for a long time
                !node
                .status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .and_then(|conditions| {
                    conditions
                        .iter()
                        .find(|condition| condition.type_ == "Ready")
                })
                .map(|condition|
                    // If the node is ready
                    condition.status == "True"
                    // If the node was ready just before 
                    || condition.last_heartbeat_time.as_ref().map(|last_heartbeat_time| {
                        Utc::now() - last_heartbeat_time.0 <= ::chrono::Duration::from_std(Self::THRESHOLD_SESSION_TIMEOUT).unwrap()
                    }).unwrap_or(false))
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

    #[instrument(level = Level::INFO, skip(self, spec), fields(node_name = %spec.node.name_any(), user_name = %spec.user_name), err(Display))]
    async fn create(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx = self.get_context(spec);

        self.label_node(ctx.spec.node, Some(ctx.spec.user_name))
            .and_then(|()| self.label_namespace(&ctx, Some(ctx.spec.user_name)))
            .and_then(|()| self.label_user(ctx.spec.node, ctx.spec.user_name, true))
            .and_then(|()| self.try_label_box(ctx.spec.node, Some(ctx.spec.user_name)))
            .and_then(|()| self.create_shared_pvc(&ctx))
            .and_then(|()| self.create_template(&ctx))
            .await
    }

    #[instrument(level = Level::INFO, skip(self, spec), fields(node_name = %spec.node.name_any(), user_name = %spec.user_name), err(Display))]
    pub async fn delete(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx = self.get_context(spec);

        self.delete_template(&ctx)
            .and_then(|()| self.delete_pods(&ctx))
            .and_then(|()| self.try_label_box(ctx.spec.node, None))
            .and_then(|()| self.label_user(ctx.spec.node, ctx.spec.user_name, false))
            .and_then(|()| self.label_namespace(&ctx, None))
            .and_then(|()| self.label_node(ctx.spec.node, None))
            .await
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
            user_name = %ctx.spec.user_name,
        ),
        err(Display),
    )]
    async fn exists_template(&self, ctx: &SessionContext<'_>) -> Result<bool> {
        self.client
            .exists_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
            user_name = %ctx.spec.user_name,
        ),
        err(Display),
    )]
    async fn create_namespace(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_named(Self::TEMPLATE_NAMESPACE_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
            user_name = %ctx.spec.user_name,
        ),
        err(Display),
    )]
    async fn create_shared_pvc(&self, ctx: &SessionContext<'_>) -> Result<()> {
        ::vine_storage::get_or_create_shared_pvcs(&self.client.kube, &ctx.metadata.namespace)
            .await
            .map(|_| ())
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
            user_name = %ctx.spec.user_name,
        ),
        err(Display),
    )]
    async fn create_template(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
            user_name = %ctx.spec.user_name,
        ),
        err(Display),
    )]
    async fn delete_template(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .delete_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
            user_name = %ctx.spec.user_name,
        ),
        err(Display),
    )]
    async fn delete_pods(&self, ctx: &SessionContext<'_>) -> Result<()> {
        let api = Api::<Pod>::namespaced(self.client.kube.clone(), &ctx.metadata.namespace);
        let dp = DeleteParams::background();
        let lp = ListParams {
            label_selector: Some("name=desktop".into()),
            ..Default::default()
        };
        api.delete_collection(&dp, &lp)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node_name = %node.name_any()), err(Display))]
    async fn try_label_box(&self, node: &Node, user_name: Option<&str>) -> Result<()> {
        let name = node.name_any();
        self.try_label::<BoxCrd>(&name, node, user_name).await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node_name = %node.name_any()), err(Display))]
    async fn try_label<K>(&self, name: &str, node: &Node, user_name: Option<&str>) -> Result<()>
    where
        K: Clone + fmt::Debug + DeserializeOwned + Resource<DynamicType = ()>,
    {
        let api = Api::<K>::all(self.client.kube.clone());
        if api.get_opt(name).await?.is_some() {
            self.label_with_api(api, name, node, user_name).await
        } else {
            Ok(())
        }
    }

    #[instrument(
        level = Level::INFO,
        skip(self, ctx),
        fields(
            name = %ctx.metadata.name,
            namespace = %ctx.metadata.namespace,
            node_name = %ctx.spec.node.name_any(),
        ),
        err(Display),
    )]
    async fn label_namespace(
        &self,
        ctx: &SessionContext<'_>,
        user_name: Option<&str>,
    ) -> Result<()> {
        self.create_namespace(ctx).await?;

        let name = self.client.namespace();
        self.label::<Namespace>(name, ctx.spec.node, user_name)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node_name = %node.name_any()), err(Display))]
    async fn label_node(&self, node: &Node, user_name: Option<&str>) -> Result<()> {
        let name = node.name_any();
        self.label::<Node>(&name, node, user_name).await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node_name = %node.name_any()), err(Display))]
    async fn label_user(&self, node: &Node, user_name: &str, create: bool) -> Result<()> {
        self.label::<UserCrd>(user_name, node, if create { Some(user_name) } else { None })
            .await
    }

    #[instrument(level = Level::INFO, skip(self, node), fields(node_name = %node.name_any()), err(Display))]
    async fn label<K>(&self, name: &str, node: &Node, user_name: Option<&str>) -> Result<()>
    where
        K: Clone + fmt::Debug + DeserializeOwned + Resource<DynamicType = ()>,
    {
        let api = Api::<K>::all(self.client.kube.clone());
        self.label_with_api(api, name, node, user_name).await
    }

    #[instrument(level = Level::INFO, skip(self, api, node), fields(node_name = %node.name_any()), err(Display))]
    async fn label_with_api<K>(
        &self,
        api: Api<K>,
        name: &str,
        node: &Node,
        user_name: Option<&str>,
    ) -> Result<()>
    where
        K: Clone + fmt::Debug + DeserializeOwned + Resource<DynamicType = ()>,
    {
        let pp = PatchParams {
            field_manager: Some(self::consts::NAME.into()),
            force: true,
            ..Default::default()
        };

        let node_name = node.name_any();
        let persistence = node
            .labels()
            .get(::ark_api::consts::LABEL_BIND_PERSISTENT)
            .and_then(|value| value.parse().ok())
            .unwrap_or_default();
        let patch = Patch::Apply(json!({
            "apiVersion": K::api_version(&()),
            "kind": K::kind(&()),
            "metadata": {
                "name": name,
                "labels": get_label(&node_name, user_name, persistence),
            },
        }));
        api.patch(name, &pp, &patch)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    fn get_context<'a>(&self, spec: &'a SessionContextSpec<'a>) -> SessionContext<'a> {
        SessionContext {
            metadata: SessionContextMetadata {
                name: "".to_string(), // not used
                namespace: self.client.namespace().to_string(),
            },
            spec,
        }
    }
}

pub type SessionContext<'a> = ::dash_provider_api::SessionContext<&'a SessionContextSpec<'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextSpecOwned {
    pub box_quota: Option<UserBoxQuotaSpec>,
    pub node: Node,
    pub persistence: bool,
    pub role: Option<UserRoleSpec>,
    pub user_name: String,
}

impl SessionContextSpecOwned {
    pub fn as_ref(&self) -> SessionContextSpec {
        SessionContextSpec {
            box_quota: self.box_quota.as_ref(),
            node: &self.node,
            persistence: self.persistence,
            role: self.role.as_ref(),
            user_name: &self.user_name,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextSpec<'a> {
    pub box_quota: Option<&'a UserBoxQuotaSpec>,
    pub node: &'a Node,
    pub persistence: bool,
    pub role: Option<&'a UserRoleSpec>,
    pub user_name: &'a str,
}

pub fn is_persistent(node: &Node) -> bool {
    node.labels()
        .get(::ark_api::consts::LABEL_BIND_PERSISTENT)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

pub fn is_persistent_by(node: &Node, user_name: &str) -> bool {
    is_persistent(node)
        && node
            .labels()
            .get(::ark_api::consts::LABEL_BIND_BY_USER)
            .map(|value| value == user_name || value.is_empty())
            .unwrap_or_default()
}

fn get_label(node_name: &str, user_name: Option<&str>, persistent: bool) -> Value {
    json!({
        ::ark_api::consts::LABEL_BIND_BY_USER: user_name,
        ::ark_api::consts::LABEL_BIND_NAMESPACE: user_name.map(UserCrd::user_namespace_with),
        ::ark_api::consts::LABEL_BIND_NODE: node_name,
        ::ark_api::consts::LABEL_BIND_PERSISTENT: persistent.to_string(),
        ::ark_api::consts::LABEL_BIND_STATUS: user_name.is_some().to_string(),
        ::ark_api::consts::LABEL_BIND_TIMESTAMP: user_name.map(|_| Utc::now().timestamp_millis().to_string()),
    })
}

pub fn is_allocable<'a>(
    labels: &'a BTreeMap<String, String>,
    node_name: &str,
    user_name: &str,
) -> AllocationState<'a> {
    let check_by_key = |key, value| labels.get(key).filter(|&label_value| label_value != value);

    if check_by_key(::ark_api::consts::LABEL_BIND_STATUS, "false").is_none() {
        AllocationState::NotAllocated
    } else if let Some(node_name) = check_by_key(::ark_api::consts::LABEL_BIND_NODE, node_name) {
        AllocationState::AllocatedByOtherNode { node_name }
    } else if let Some(user_name) = check_by_key(::ark_api::consts::LABEL_BIND_BY_USER, user_name) {
        AllocationState::AllocatedByOtherUser { user_name }
    } else {
        AllocationState::AllocatedByMyself
    }
}

pub enum AllocationState<'a> {
    AllocatedByMyself,
    AllocatedByOtherNode { node_name: &'a str },
    AllocatedByOtherUser { user_name: &'a str },
    NotAllocated,
}
