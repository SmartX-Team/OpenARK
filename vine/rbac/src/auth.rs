use std::collections::BTreeMap;

use anyhow::Result;
use chrono::Utc;
use k8s_openapi::api::core::v1::Node;
use kube::{api::ListParams, Api, Client, ResourceExt};
use log::{info, warn};
use vine_api::{
    user::{UserCrd, UserSpec},
    user_auth::{UserAuthError, UserAuthResponse},
    user_box_binding::{UserBoxBindingCrd, UserBoxBindingSpec},
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::{UserBoxQuotaBindingCrd, UserBoxQuotaBindingSpec},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserSessionRef {
    pub box_name: Option<String>,
    pub namespace: String,
    pub user: UserSpec,
    pub user_name: String,
}

impl UserSessionRef {
    #[cfg(feature = "actix")]
    pub async fn from_request(
        client: &::kube::Client,
        request: &::actix_web::HttpRequest,
    ) -> Result<Self> {
        use anyhow::{anyhow, bail};

        let user_name =
            get_user_name(request).map_err(|error| anyhow!("failed to get user name: {error}"))?;

        let namespace = get_user_namespace_with(client, request, &user_name)
            .await
            .map_err(|error| anyhow!("failed to get user namespace: {error}"))?;

        execute(client, &user_name)
            .await
            .and_then(|response| match response {
                UserAuthResponse::Accept { box_name, user, .. } => Ok(Self {
                    box_name,
                    namespace,
                    user,
                    user_name,
                }),
                UserAuthResponse::Error(error) => bail!("failed to auth user: {error}"),
            })
    }
}

#[cfg(feature = "actix")]
pub fn get_user_name(request: &::actix_web::HttpRequest) -> Result<String, UserAuthError> {
    use anyhow::{bail, Error};
    use base64::Engine;
    use vine_api::user_auth::UserAuthPayload;

    // get current time
    let now = Utc::now();

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
                None => bail!("[{now}] the Authorization token is not a Bearer token"),
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
pub async fn get_user_namespace(
    client: &::kube::Client,
    request: &::actix_web::HttpRequest,
) -> Result<String, UserAuthError> {
    get_user_namespace_with(client, request, &get_user_name(request)?).await
}

#[cfg(feature = "actix")]
pub(crate) async fn get_user_namespace_with(
    client: &::kube::Client,
    request: &::actix_web::HttpRequest,
    user_name: &str,
) -> Result<String, UserAuthError> {
    use anyhow::Error;
    use vine_api::{
        user_role::{UserRoleCrd, UserRoleSpec},
        user_role_binding::UserRoleBindingCrd,
    };

    // get current time
    let now = Utc::now();

    match request.headers().get(::ark_api::consts::HEADER_NAMESPACE) {
        Some(token) => match token.to_str().map_err(Error::from) {
            Ok(namespace) => {
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
                        .fold(UserRoleSpec::default(), |a, b| (a | *b))
                };

                if role.is_admin {
                    Ok(namespace.into())
                } else {
                    Err(UserAuthError::NamespaceNotAllowed)
                }
            }
            Err(e) => {
                warn!("[{now}] failed to parse the token: {token:?}: {e}");
                Err(UserAuthError::NamespaceTokenMalformed)
            }
        },
        None => Ok(UserCrd::user_namespace_with(user_name)),
    }
}

pub async fn execute(client: &Client, user_name: &str) -> Result<UserAuthResponse> {
    // get current time
    let now = Utc::now();

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

    // Login Successed!
    info!("[{now}] auth accepted: {user_name:?}");
    Ok(UserAuthResponse::Accept {
        box_bindings,
        box_name: user
            .labels()
            .get(::ark_api::consts::LABEL_BIND_NODE)
            .cloned(),
        box_quota_bindings,
        user: user.spec,
    })
}
