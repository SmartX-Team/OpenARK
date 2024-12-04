use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use ark_api::SessionRef;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
#[cfg(feature = "actix")]
use k8s_openapi::api::core::v1::Node;
#[cfg(feature = "actix")]
use kube::Client;
use kube::{api::ListParams, Api, ResourceExt};
#[cfg(feature = "actix")]
use tracing::warn;
use tracing::{instrument, Level};
use vine_api::{
    user::UserCrd,
    user_auth::UserAuthError,
    user_role::{UserRoleCrd, UserRoleSpec},
    user_role_binding::UserRoleBindingCrd,
    user_session::UserSession,
};
#[cfg(feature = "actix")]
use vine_api::{
    user_box_binding::{UserBoxBindingCrd, UserBoxBindingSpec},
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::{UserBoxQuotaBindingCrd, UserBoxQuotaBindingSpec},
};

#[async_trait(?Send)]
pub trait AuthUserSession {
    fn assert_admin(&self) -> Result<(), UserAuthError> {
        if self.role().is_admin {
            Ok(())
        } else {
            Err(UserAuthError::UserNotAdmin)
        }
    }

    fn role(&self) -> &UserRoleSpec;

    #[cfg(feature = "actix")]
    #[instrument(level = Level::INFO, skip(client, request), err(Display))]
    async fn from_request(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
    ) -> Result<Self, UserAuthError>
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
    ) -> Result<Self, UserAuthError>
    where
        Self: Sized;

    async fn namespaced(self, namespace: Option<String>) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait(?Send)]
impl AuthUserSession for UserSession {
    fn role(&self) -> &UserRoleSpec {
        &self.role
    }

    #[cfg(feature = "actix")]
    #[instrument(level = Level::INFO, skip(client, request), err(Display))]
    async fn from_request_with_timestamp(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
        now: DateTime<Utc>,
    ) -> Result<Self, UserAuthError>
    where
        Self: Sized,
    {
        use std::sync::Arc;

        use tracing::info;

        let token = get_user_token(request)?;
        let user_name = get_user_name_with_timestamp_impl(&token, now)?;

        let api = Api::<UserCrd>::all(client.clone());
        let user = api.get(&user_name).await.map_err(|e| {
            warn!("[{now}] failed to find the user: {e}");
            UserAuthError::UserNotRegistered
        })?;
        let user_name = user.preferred_name();

        let role = get_user_role(client, &user, now).await?;
        let namespace = get_user_namespace(request, &user, role, now)?;

        let config = ::kube::Config {
            auth_info: ::kube::config::AuthInfo {
                token: Some(token.to_string().into()),
                ..Default::default()
            },
            default_namespace: namespace.clone(),
            ..::kube::Config::incluster().map_err(|_| UserAuthError::NamespaceNotAllowed)?
        };
        let client = Client::try_from(config).map_err(|_| UserAuthError::NamespaceNotAllowed)?;

        // get available boxes
        let boxes = {
            let api = Api::<Node>::all(client.clone());
            let lp = ListParams::default();
            api.list(&lp)
                .await
                .map(|list| {
                    list.items
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
                })
                .unwrap_or_default()
        };

        let box_bindings = {
            let api = Api::<UserBoxBindingCrd>::all(client.clone());
            let lp = ListParams::default();
            api.list(&lp)
                .await
                .map(|list| {
                    list.items
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
                })
                .unwrap_or_default()
        };

        // get available quotas
        let quotas = {
            let api = Api::<UserBoxQuotaCrd>::all(client.clone());
            let lp = ListParams::default();
            api.list(&lp)
                .await
                .map(|list| {
                    list.items
                        .into_iter()
                        .map(|item| (item.name_any(), item.spec))
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default()
        };

        let box_quota_bindings = {
            let api = Api::<UserBoxQuotaBindingCrd>::all(client.clone());
            let lp = ListParams::default();
            api.list(&lp)
                .await
                .map(|list| {
                    list.items
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
                })
                .unwrap_or_default()
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

        Ok(Self {
            box_bindings: Arc::new(box_bindings),
            box_name,
            box_quota_bindings: Arc::new(box_quota_bindings),
            kube: Some(client),
            namespace,
            token: Some(token.into()),
            role,
            user: Arc::new(user),
            user_name,
        })
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn namespaced(self, namespace: Option<String>) -> Result<Self> {
        let Self {
            box_bindings,
            box_name,
            box_quota_bindings,
            kube,
            namespace: _,
            role,
            token,
            user,
            user_name,
        } = self;

        check_user_namespace(namespace, &user, role)
            .map(|namespace| Self {
                box_bindings,
                box_name,
                box_quota_bindings,
                kube,
                namespace,
                role,
                token,
                user,
                user_name,
            })
            .map_err(|error| anyhow!("failed to get user namespace: {error}"))
    }
}

pub trait AuthUserSessionRef {
    fn try_into_ark_session(self) -> Result<SessionRef<'static>, UserAuthError>;
}

impl AuthUserSessionRef for UserSession {
    fn try_into_ark_session(self) -> Result<SessionRef<'static>, UserAuthError> {
        let Self {
            box_name,
            namespace,
            user_name,
            ..
        } = self;

        Ok(SessionRef {
            namespace: namespace.into(),
            node_name: match box_name {
                Some(box_name) => box_name.into(),
                None => return Err(UserAuthError::SessionNotBinded),
            },
            timestamp: None,
            user_name: user_name.into(),
        })
    }
}

#[cfg(feature = "actix")]
pub fn get_user_name(request: &::actix_web::HttpRequest) -> Result<String, UserAuthError> {
    // get current time
    let now = Utc::now();
    get_user_name_with_timestamp(request, now)
}

#[cfg(all(feature = "actix", feature = "unsafe-mock"))]
fn get_user_name_with_timestamp(
    request: &::actix_web::HttpRequest,
    now: DateTime<Utc>,
) -> Result<String, UserAuthError> {
    ::std::env::var("DASH_UNSAFE_MOCK_USERNAME").or_else(|_| {
        get_user_token(request).and_then(|token| get_user_name_with_timestamp_impl(&token, now))
    })
}

#[cfg(all(feature = "actix", not(feature = "unsafe-mock")))]
#[inline]
fn get_user_name_with_timestamp(
    request: &::actix_web::HttpRequest,
    now: DateTime<Utc>,
) -> Result<String, UserAuthError> {
    let token = get_user_token(request)?;
    get_user_name_with_timestamp_impl(&token, now)
}

#[cfg(feature = "actix")]
fn get_user_name_with_timestamp_impl(
    token: &str,
    now: DateTime<Utc>,
) -> Result<String, UserAuthError> {
    use base64::Engine;
    use vine_api::user_auth::UserAuthPayload;

    // parse the Authorization token
    let payload: UserAuthPayload = match match token.split('.').nth(1) {
        Some(payload) => ::base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(payload)
            .map_err(Into::into)
            .and_then(|payload| ::serde_json::from_slice(&payload).map_err(Into::into)),
        None => Err(::anyhow::anyhow!(
            "[{now}] the Authorization token is not a Bearer token"
        )),
    } {
        Ok(payload) => payload,
        Err(e) => {
            warn!("[{now}] failed to parse the token: {token:?}: {e}");
            return Err(UserAuthError::AuthorizationTokenMalformed);
        }
    };

    // get the user primary key
    payload.primary_key().map_err(|e| {
        warn!("[{now}] failed to parse the user's primary key: {payload:?}: {e}");
        UserAuthError::PrimaryKeyMalformed
    })
}

#[cfg(feature = "actix")]
#[instrument(level = Level::INFO, skip(request), err(Display))]
fn get_user_namespace(
    request: &::actix_web::HttpRequest,
    user: &UserCrd,
    role: UserRoleSpec,
    now: DateTime<Utc>,
) -> Result<String, UserAuthError> {
    match request.headers().get(::ark_api::consts::HEADER_NAMESPACE) {
        Some(token) => match token.to_str().map_err(::anyhow::Error::from) {
            Ok(namespace) => check_user_namespace(Some(namespace.into()), user, role),
            Err(e) => {
                warn!("[{now}] failed to parse the token: {token:?}: {e}");
                Err(UserAuthError::NamespaceTokenMalformed)
            }
        },
        None => Ok(user.user_namespace()),
    }
}

#[instrument(level = Level::INFO, skip(client), err(Display))]
async fn get_user_role(
    client: &::kube::Client,
    user: &UserCrd,
    now: DateTime<Utc>,
) -> Result<UserRoleSpec, UserAuthError> {
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
        let user_name = user.preferred_name();
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

#[cfg(feature = "actix")]
fn get_user_token(request: &::actix_web::HttpRequest) -> Result<&str, UserAuthError> {
    const HEADER_AUTHORIZATION: &str = "Authorization";

    request
        .headers()
        .get(HEADER_AUTHORIZATION)
        .ok_or(UserAuthError::AuthorizationTokenNotFound)
        .and_then(|token| {
            token
                .to_str()
                .map_err(|_| UserAuthError::AuthorizationTokenMalformed)
        })
        .and_then(|token| {
            token
                .strip_prefix("Bearer ")
                .ok_or(UserAuthError::AuthorizationTokenNotFound)
        })
}

fn check_user_namespace(
    namespace: Option<String>,
    user: &UserCrd,
    role: UserRoleSpec,
) -> Result<String, UserAuthError> {
    match namespace {
        Some(namespace) => {
            if role.is_admin || namespace == user.user_namespace() {
                Ok(namespace)
            } else {
                Err(UserAuthError::NamespaceNotAllowed)
            }
        }
        None => Ok(user.user_namespace()),
    }
}
