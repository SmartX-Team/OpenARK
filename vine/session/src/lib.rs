use std::{collections::BTreeMap, fmt, fs, path::PathBuf, time::Duration};

use anyhow::{bail, Error, Result};
use ark_api::{NamespaceAny, SessionRef};
use ark_core::env;
use chrono::Utc;
use dash_provider::client::job::FunctionActorJobClient;
use dash_provider_api::SessionContextMetadata;
use futures::TryFutureExt;
use k8s_openapi::{
    api::core::v1::{Namespace, Node, Pod, PodCondition},
    serde_json::Value,
};
use kube::{
    api::{DeleteParams, ListParams, Patch, PatchParams},
    Api, Client, Resource, ResourceExt,
};
use log::info;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use vine_api::{user::UserCrd, user_box_quota::UserBoxQuotaSpec, user_role::UserRoleSpec};

pub(crate) mod consts {
    pub const NAME: &str = "vine-session";
}

pub struct SessionManager {
    client: FunctionActorJobClient,
}

impl SessionManager {
    pub async fn try_new(namespace: String, kube: Client) -> Result<Self> {
        let templates_home = env::infer("VINE_SESSION_TEMPLATES_HOME").or_else(|_| {
            // local directory
            "../../templates/vine/templates/session"
                .parse::<PathBuf>()
                .map_err(Error::from)
        })?;
        let templates_home = fs::canonicalize(templates_home)?;

        match templates_home.to_str() {
            Some(templates_home) => {
                let metadata = Default::default();
                let templates_home = format!("{templates_home}/*.yaml.j2");
                let use_prefix = false;
                let client = FunctionActorJobClient::from_dir(metadata, namespace, kube, &templates_home, use_prefix)?;
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

    pub async fn try_create(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        match self.create(spec).await {
            Ok(()) => Ok(()),
            Err(error_create) => match self.delete(spec).await {
                Ok(()) => Err(error_create),
                Err(error_revert) => bail!("{error_create}\n{error_revert}"),
            },
        }
    }

    pub async fn try_delete(&self, node: &Node) -> Result<Option<String>> {
        match node.get_session_ref() {
            Ok(SessionRef { user_name, .. }) => {
                let spec = SessionContextSpec {
                    box_quota: None,
                    node,
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

    async fn create(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx = self.get_context(spec);

        self.label_node(ctx.spec.node, Some(ctx.spec.user_name))
            .and_then(|()| self.label_namespace(&ctx, Some(ctx.spec.user_name)))
            .and_then(|()| self.label_user(ctx.spec.node, ctx.spec.user_name, true))
            .and_then(|()| self.create_shared_pvc(&ctx))
            .and_then(|()| self.create_template(&ctx))
            .await
    }

    pub async fn delete(&self, spec: &SessionContextSpec<'_>) -> Result<()> {
        let ctx = self.get_context(spec);

        self.delete_template(&ctx)
            .and_then(|()| self.delete_pods(&ctx))
            .and_then(|()| self.label_user(ctx.spec.node, ctx.spec.user_name, false))
            .and_then(|()| self.label_namespace(&ctx, None))
            .and_then(|()| self.label_node(ctx.spec.node, None))
            .await
    }

    async fn exists_template(&self, ctx: &SessionContext<'_>) -> Result<bool> {
        self.client
            .exists_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
    }

    async fn create_namespace(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_named(Self::TEMPLATE_NAMESPACE_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn create_shared_pvc(&self, ctx: &SessionContext<'_>) -> Result<()> {
        ::vine_storage::get_or_create_shared_pvcs(&self.client.kube, &ctx.metadata.namespace)
            .await
            .map(|_| ())
    }

    async fn create_template(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .create_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
            .map(|_| ())
    }

    async fn delete_template(&self, ctx: &SessionContext<'_>) -> Result<()> {
        self.client
            .delete_named(Self::TEMPLATE_SESSION_FILENAME, ctx)
            .await
            .map(|_| ())
    }

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

    async fn label_node(&self, node: &Node, user_name: Option<&str>) -> Result<()> {
        let name = node.name_any();
        self.label::<Node>(&name, node, user_name).await
    }

    async fn label_user(&self, node: &Node, user_name: &str, create: bool) -> Result<()> {
        self.label::<UserCrd>(user_name, node, if create { Some(user_name) } else { None })
            .await
    }

    async fn label<K>(&self, name: &str, node: &Node, user_name: Option<&str>) -> Result<()>
    where
        K: Clone + fmt::Debug + DeserializeOwned + Resource<DynamicType = ()>,
    {
        let api = Api::<K>::all(self.client.kube.clone());
        let pp = PatchParams {
            field_manager: Some(self::consts::NAME.into()),
            force: true,
            ..Default::default()
        };

        let node_name = node.name_any();
        let patch = Patch::Apply(json!({
            "apiVersion": K::api_version(&()),
            "kind": K::kind(&()),
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

    async fn exec<I, T>(
        &self,
        kube: Client,
        command: I,
    ) -> Result<Vec<::kube::api::AttachedProcess>>
    where
        I: Send + Sync + Clone + fmt::Debug + IntoIterator<Item = T>,
        T: Sync + Into<String>;
}

#[cfg(feature = "exec")]
#[::async_trait::async_trait]
impl<'a> SessionExec for SessionRef<'a> {
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
            .map(|users| {
                users
                    .into_iter()
                    .filter_map(|user| {
                        user.get_session_ref()
                            .map(|session| session.into_owned())
                            .ok()
                    })
                    .collect()
            })
            .map_err(Into::into)
    }

    async fn exec<I, T>(
        &self,
        kube: Client,
        command: I,
    ) -> Result<Vec<::kube::api::AttachedProcess>>
    where
        I: Send + Sync + Clone + fmt::Debug + IntoIterator<Item = T>,
        T: Sync + Into<String>,
    {
        use futures::future::try_join_all;
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

        try_join_all(pods.into_iter().map(|pod| {
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
        }))
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
    pub role: Option<UserRoleSpec>,
    pub user_name: String,
}

impl SessionContextSpecOwned {
    pub fn as_ref(&self) -> SessionContextSpec {
        SessionContextSpec {
            box_quota: self.box_quota.as_ref(),
            node: &self.node,
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
    pub role: Option<&'a UserRoleSpec>,
    pub user_name: &'a str,
}

fn get_label(node_name: &str, user_name: Option<&str>) -> Value {
    json!({
        ::ark_api::consts::LABEL_BIND_BY_USER: user_name,
        ::ark_api::consts::LABEL_BIND_NAMESPACE: user_name.map(UserCrd::user_namespace_with),
        ::ark_api::consts::LABEL_BIND_NODE: node_name,
        ::ark_api::consts::LABEL_BIND_PERSISTENT: "false",
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
