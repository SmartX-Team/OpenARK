use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use ark_api::SessionRef;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Node;
use kube::{api::ListParams, Api, Client, ResourceExt};
use tracing::{info, instrument, warn, Level};
use vine_api::{
    user::UserCrd,
    user_auth::{UserAuthError, UserAuthResponse},
    user_box_binding::{UserBoxBindingCrd, UserBoxBindingSpec},
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::{UserBoxQuotaBindingCrd, UserBoxQuotaBindingSpec},
    user_role::UserRoleSpec,
    user_session::{UserSessionMetadata, UserSessionRef},
};

#[async_trait(?Send)]
pub trait AuthUserSession {
    fn assert_admin(&self) -> Result<()> {
        if self.role().is_admin {
            Ok(())
        } else {
            bail!("user it not an admin")
        }
    }

    fn role(&self) -> &UserRoleSpec;

    #[cfg(feature = "actix")]
    #[instrument(level = Level::INFO, skip(client, request), err(Display))]
    async fn from_request(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        // get current time
        let now = Utc::now();

        Self::from_request_with_timestamp(client, request, now).await
    }

    #[cfg(feature = "actix")]
    async fn from_request_with_timestamp(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
        now: DateTime<Utc>,
    ) -> Result<Self>
    where
        Self: Sized;
}

pub trait AuthUserSessionRef {
    fn try_into_ark_session(self) -> Result<SessionRef<'static>>;
}

#[async_trait(?Send)]
impl AuthUserSession for UserSessionRef {
    fn role(&self) -> &UserRoleSpec {
        self.metadata.role()
    }

    #[cfg(feature = "actix")]
    #[instrument(level = Level::INFO, skip(client, request), err(Display))]
    async fn from_request_with_timestamp(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
        now: DateTime<Utc>,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        let metadata =
            UserSessionMetadata::from_request_with_timestamp(client, request, now).await?;

        get_user_namespace_with(request, &metadata.user_name, metadata.role, now)
            .map(|namespace| Self {
                metadata,
                namespace,
            })
            .map_err(|error| anyhow!("failed to get user namespace: {error}"))
    }
}

impl AuthUserSessionRef for UserSessionRef {
    fn try_into_ark_session(self) -> Result<SessionRef<'static>> {
        let Self {
            metadata:
                UserSessionMetadata {
                    box_name,
                    role: _,
                    user: _,
                    user_name,
                },
            namespace,
        } = self;

        Ok(SessionRef {
            namespace: namespace.into(),
            node_name: match box_name {
                Some(box_name) => box_name.into(),
                None => bail!("session is not binded: {user_name}"),
            },
            user_name: user_name.into(),
        })
    }
}

#[async_trait(?Send)]
pub trait AuthUserSessionMetadata {
    async fn namespaced(&self, namespace: Option<String>) -> Result<UserSessionRef>;
}

#[async_trait(?Send)]
impl AuthUserSession for UserSessionMetadata {
    fn role(&self) -> &UserRoleSpec {
        &self.role
    }

    #[cfg(feature = "actix")]
    #[instrument(level = Level::INFO, skip(client, request), err(Display))]
    async fn from_request_with_timestamp(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
        now: DateTime<Utc>,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        let user_name = get_user_name_with_timestamp(request, now)
            .map_err(|error| anyhow!("failed to get user name: {error}"))?;

        let role = get_user_role(client, &user_name, now)
            .await
            .map_err(|error| anyhow!("failed to get user role: {error}"))?;

        execute_with_timestamp(client, &user_name, now)
            .await
            .and_then(|response| match response {
                UserAuthResponse::Accept { box_name, user, .. } => Ok(Self {
                    box_name,
                    role,
                    user,
                    user_name,
                }),
                UserAuthResponse::Error(error) => bail!("failed to auth user: {error}"),
            })
    }
}

#[async_trait(?Send)]
impl AuthUserSessionMetadata for UserSessionMetadata {
    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn namespaced(&self, namespace: Option<String>) -> Result<UserSessionRef> {
        check_user_namespace(namespace, &self.user_name, self.role)
            .map(|namespace| UserSessionRef {
                metadata: self.clone(),
                namespace,
            })
            .map_err(|error| anyhow!("failed to get user namespace: {error}"))
    }
}

#[cfg(feature = "actix")]
pub fn get_user_name(request: &::actix_web::HttpRequest) -> Result<String, UserAuthError> {
    // get current time
    let now = Utc::now();
    get_user_name_with_timestamp(request, now)
}

#[cfg(feature = "actix")]
fn get_user_name_with_timestamp(
    request: &::actix_web::HttpRequest,
    now: ::chrono::DateTime<Utc>,
) -> Result<String, UserAuthError> {
    use anyhow::Error;
    use base64::Engine;
    use vine_api::user_auth::UserAuthPayload;

    // parse the Authorization token
    let payload: UserAuthPayload = match request.headers().get("Authorization") {
        Some(token) => match token.to_str().map_err(Error::from).and_then(|token| {
            match token
                .strip_prefix("Bearer ")
                .and_then(|token| token.split('.').nth(1))
            {
                Some(payload) => ::base64::engine::general_purpose::STANDARD_NO_PAD
                    .decode(payload)
                    .map_err(Into::into)
                    .and_then(|payload| ::serde_json::from_slice(&payload).map_err(Into::into)),
                None => ::anyhow::bail!("[{now}] the Authorization token is not a Bearer token"),
            }
        }) {
            Ok(payload) => payload,
            Err(e) => {
                warn!("[{now}] failed to parse the token: {token:?}: {e}");
                return Err(UserAuthError::AuthorizationTokenMalformed);
            }
        },
        None => {
            warn!("[{now}] failed to get the token: Authorization");
            return Err(UserAuthError::AuthorizationTokenNotFound);
        }
    };

    // get the user primary key
    payload.primary_key().map_err(|e| {
        warn!("[{now}] failed to parse the user's primary key: {payload:?}: {e}");
        UserAuthError::PrimaryKeyMalformed
    })
}

#[cfg(feature = "actix")]
#[instrument(level = Level::INFO, skip(client, request), err(Display))]
pub async fn get_user_namespace(
    client: &::kube::Client,
    request: &::actix_web::HttpRequest,
) -> Result<String, UserAuthError> {
    // get current time
    let now = Utc::now();

    let user_name = get_user_name(request)?;
    let role = get_user_role(client, &user_name, now).await?;
    get_user_namespace_with(request, &user_name, role, now)
}

#[cfg(feature = "actix")]
fn get_user_namespace_with(
    request: &::actix_web::HttpRequest,
    user_name: &str,
    role: UserRoleSpec,
    now: DateTime<Utc>,
) -> Result<String, UserAuthError> {
    match request.headers().get(::ark_api::consts::HEADER_NAMESPACE) {
        Some(token) => match token.to_str().map_err(::anyhow::Error::from) {
            Ok(namespace) => check_user_namespace(Some(namespace.into()), user_name, role),
            Err(e) => {
                warn!("[{now}] failed to parse the token: {token:?}: {e}");
                Err(UserAuthError::NamespaceTokenMalformed)
            }
        },
        None => Ok(UserCrd::user_namespace_with(user_name)),
    }
}

#[instrument(level = Level::INFO, skip(client), err(Display))]
async fn get_user_role(
    client: &::kube::Client,
    user_name: &str,
    now: ::chrono::DateTime<Utc>,
) -> Result<UserRoleSpec, UserAuthError> {
    use vine_api::{user_role::UserRoleCrd, user_role_binding::UserRoleBindingCrd};

    // get available roles
    let roles = {
        let api = Api::<UserRoleCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await
            .map(|list| list.items)
            .unwrap_or_else(|_| Default::default())
            .into_iter()
            .map(|item| (item.name_any(), item.spec))
            .collect::<BTreeMap<_, _>>()
    };

    let role = {
        let api = Api::<UserRoleBindingCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await
            .map(|list| list.items)
            .unwrap_or_else(|_| Default::default())
            .into_iter()
            .filter(|item| item.spec.user == user_name)
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .filter_map(|item| roles.get(&item.spec.role))
            .copied()
            .sum()
    };
    Ok(role)
}

#[instrument(level = Level::INFO, skip(client), err(Display))]
pub async fn execute(client: &Client, user_name: &str) -> Result<UserAuthResponse> {
    // get current time
    let now = Utc::now();
    execute_with_timestamp(client, user_name, now).await
}

#[instrument(level = Level::INFO, skip(client), err(Display))]
async fn execute_with_timestamp(
    client: &Client,
    user_name: &str,
    now: DateTime<Utc>,
) -> Result<UserAuthResponse> {
    // get the user CR
    let api = Api::<UserCrd>::all(client.clone());
    let user = match api.get_opt(user_name).await? {
        Some(user) => user,
        None => {
            warn!("[{now}] failed to find an user: {user_name:?}");
            return Ok(UserAuthError::UserNotRegistered.into());
        }
    };

    // get available boxes
    let boxes = {
        let api = Api::<Node>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| {
                item.status
                    .as_ref()
                    .and_then(|status| status.conditions.as_ref())
                    .and_then(|conditions| conditions.last())
                    .map(|condition| condition.status == "True")
                    .unwrap_or_default()
            })
            .map(|item| (item.name_any(), item.spec.unwrap()))
            .collect::<BTreeMap<_, _>>()
    };

    let box_bindings = {
        let api = Api::<UserBoxBindingCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| item.spec.user == user_name)
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .filter_map(|item| {
                Some(UserBoxBindingSpec {
                    user: item.spec.user,
                    r#box: boxes.get(&item.spec.r#box)?.clone(),
                    autologin: item.spec.autologin,
                    expired_timestamp: item.spec.expired_timestamp,
                })
            })
            .collect::<Vec<_>>()
    };

    // get available quotas
    let quotas = {
        let api = Api::<UserBoxQuotaCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .map(|item| (item.name_any(), item.spec))
            .collect::<BTreeMap<_, _>>()
    };

    let box_quota_bindings = {
        let api = Api::<UserBoxQuotaBindingCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| item.spec.user == user_name)
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .filter_map(|item| {
                Some(UserBoxQuotaBindingSpec {
                    user: item.spec.user,
                    quota: quotas.get(&item.spec.quota)?.clone(),
                    expired_timestamp: item.spec.expired_timestamp,
                })
            })
            .collect::<Vec<_>>()
    };

    // get current box info
    let labels = user.labels();
    let box_name = if labels
        .get(::ark_api::consts::LABEL_BIND_STATUS)
        .map(AsRef::as_ref)
        == Some("true")
    {
        labels.get(::ark_api::consts::LABEL_BIND_NODE).cloned()
    } else {
        None
    };

    // Login Successed!
    info!("[{now}] auth accepted: {user_name:?}");
    Ok(UserAuthResponse::Accept {
        box_bindings,
        box_name,
        box_quota_bindings,
        user: user.spec,
    })
}

fn check_user_namespace(
    namespace: Option<String>,
    user_name: &str,
    role: UserRoleSpec,
) -> Result<String, UserAuthError> {
    match namespace {
        Some(namespace) => {
            if role.is_admin || namespace == UserCrd::user_namespace_with(user_name) {
                Ok(namespace)
            } else {
                Err(UserAuthError::NamespaceNotAllowed)
            }
        }
        None => Ok(UserCrd::user_namespace_with(user_name)),
    }
}
