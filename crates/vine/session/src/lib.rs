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

#[cfg(feature = "batch")]
pub struct BatchCommandArgs<C, U> {
    pub command: C,
    pub users: BatchCommandUsers<U>,
    pub wait: bool,
}

#[cfg(feature = "batch")]
pub enum BatchCommandUsers<U> {
    All,
    List(Vec<U>),
    Pattern(U),
}

#[cfg(feature = "batch")]
impl<U> BatchCommandUsers<U>
where
    U: AsRef<str>,
{
    fn filter(
        &self,
        sessions_all: impl Iterator<Item = SessionRef<'static>>,
    ) -> Result<Vec<SessionRef<'static>>> {
        use anyhow::anyhow;

        match self {
            Self::All => Ok(sessions_all.collect()),
            Self::List(items) => Ok(sessions_all
                .filter(|session| items.iter().any(|item| item.as_ref() == session.user_name))
                .collect()),
            Self::Pattern(re) => {
                let re = ::regex::Regex::new(re.as_ref())
                    .map_err(|error| anyhow!("failed to parse box regex pattern: {error}"))?;

                Ok(sessions_all
                    .filter(|session| re.is_match(&session.user_name))
                    .collect())
            }
        }
    }
}

#[cfg(feature = "batch")]
impl<C, U> BatchCommandArgs<C, U> {
    pub async fn exec(&self, kube: &Client) -> Result<usize>
    where
        C: Send + Sync + Clone + fmt::Debug + IntoIterator,
        <C as IntoIterator>::Item: Sync + Into<String>,
        U: AsRef<str>,
    {
        use anyhow::anyhow;
        use futures::{stream::FuturesUnordered, StreamExt};
        use tracing::{debug, error, warn};

        let Self {
            command,
            users,
            wait,
        } = self;

        let sessions_all = {
            let api = Api::<Node>::all(kube.clone());
            let lp = ListParams::default();
            api.list_metadata(&lp)
                .await
                .map(|list| {
                    list.items
                        .into_iter()
                        .filter_map(|item| match item.get_session_ref() {
                            Ok(session) => Some(session.into_owned()),
                            Err(error) => {
                                let name = item.name_any();
                                debug!("failed to get session {name}: {error}");
                                None
                            }
                        })
                })
                .map_err(|error| anyhow!("failed to list nodes: {error}"))?
        };

        let sessions_filtered = users.filter(sessions_all)?;

        let sessions_exec = sessions_filtered
            .iter()
            .map(|session| async move { session.exec(kube.clone(), command.clone()).await });

        ::futures::stream::iter(sessions_exec)
            .collect::<FuturesUnordered<_>>()
            .await
            .then(|result| async move {
                match result {
                    Ok(processes) => {
                        if *wait {
                            ::futures::stream::iter(processes.into_iter().map(
                                |process| async move {
                                    match process.join().await {
                                        Ok(()) => (),
                                        Err(error) => {
                                            error!("failed to execute: {error}");
                                        }
                                    }
                                },
                            ))
                            .collect::<FuturesUnordered<_>>()
                            .await
                            .collect::<()>()
                            .await;
                        }
                    }
                    Err(error) => {
                        warn!("failed to command: {error}");
                    }
                }
            })
            .collect::<()>()
            .await;
        Ok(sessions_filtered.len())
    }
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

#[cfg(feature = "exec")]
#[::async_trait::async_trait]
pub trait SessionExec {
    async fn list(kube: Client) -> Result<Vec<Self>>
    where
        Self: Sized;

    async fn load<Item>(kube: Client, user_names: &[Item]) -> Result<Vec<Self>>
    where
        Self: Sized,
        Item: Send + Sync + AsRef<str>,
        [Item]: fmt::Debug;

    async fn exec<I>(&self, kube: Client, command: I) -> Result<Vec<::kube::api::AttachedProcess>>
    where
        I: Send + Sync + Clone + fmt::Debug + IntoIterator,
        <I as IntoIterator>::Item: Sync + Into<String>;
}

#[cfg(feature = "exec")]
#[::async_trait::async_trait]
impl<'a> SessionExec for SessionRef<'a> {
    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    async fn list(kube: Client) -> Result<Vec<Self>> {
        let api = Api::<UserCrd>::all(kube);
        let lp = ListParams {
            label_selector: Some(format!(
                "{key}=true",
                key = ::ark_api::consts::LABEL_BIND_STATUS,
            )),
            ..Default::default()
        };

        api.list(&lp)
            .await
            .map(collect_user_sessions)
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    async fn load<Item>(kube: Client, user_names: &[Item]) -> Result<Vec<Self>>
    where
        Item: Send + Sync + AsRef<str>,
        [Item]: fmt::Debug,
    {
        use futures::{stream::FuturesUnordered, TryStreamExt};

        let api = Api::<UserCrd>::all(kube);

        user_names
            .iter()
            .map(|user_name| api.get(user_name.as_ref()))
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
            .map(|users: Vec<_>| collect_user_sessions(users))
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, skip(kube, command), err(Display))]
    async fn exec<I>(&self, kube: Client, command: I) -> Result<Vec<::kube::api::AttachedProcess>>
    where
        I: Send + Sync + Clone + fmt::Debug + IntoIterator,
        <I as IntoIterator>::Item: Sync + Into<String>,
    {
        use futures::{stream::FuturesUnordered, TryStreamExt};
        use k8s_openapi::api::core::v1::PodCondition;
        use kube::api::AttachParams;

        let api = Api::<Pod>::namespaced(kube, &self.namespace);
        let lp = ListParams {
            label_selector: Some("name=desktop".into()),
            ..Default::default()
        };
        let pods = api.list(&lp).await?.into_iter().filter(|pod| {
            fn check_condition(conditions: &[PodCondition], type_: &str) -> bool {
                conditions
                    .iter()
                    .find(|condition| condition.type_ == type_)
                    .map(|condition| condition.status == "True")
                    .unwrap_or_default()
            }

            pod.status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .map(|conditions| {
                    check_condition(conditions, "PodScheduled") // Running
                        && !check_condition(conditions, "DisruptionTarget") // not Terminating
                })
                .unwrap_or_default()
        });

        pods.into_iter()
            .map(|pod| {
                let api = api.clone();
                let ap = AttachParams {
                    container: Some("desktop-environment".into()),
                    stdin: false,
                    stdout: true,
                    stderr: true,
                    tty: false,
                    ..Default::default()
                };
                let command = command.clone();
                async move { api.exec(&pod.name_any(), command, &ap).await }
            })
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
            .map_err(Into::into)
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

#[cfg(feature = "exec")]
fn collect_user_sessions<I>(users: I) -> Vec<SessionRef<'static>>
where
    I: IntoIterator<Item = UserCrd>,
{
    users
        .into_iter()
        .filter_map(|user| {
            user.get_session_ref()
                .map(|session| session.into_owned())
                .ok()
        })
        .collect()
}
