use std::{collections::BTreeMap, future::Future};

use anyhow::Result;
use chrono::Utc;
use k8s_openapi::api::core::v1::Node;
use kiss_api::r#box::BoxCrd;
use kube::{api::ListParams, Api, Client, ResourceExt};
use log::warn;
use vine_api::{
    user::UserCrd,
    user_auth::{UserAuthError, UserSessionResponse},
    user_box_binding::UserBoxBindingCrd,
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::UserBoxQuotaBindingCrd,
    user_role::UserRoleCrd,
    user_role_binding::UserRoleBindingCrd,
};
use vine_session::{SessionContextSpec, SessionManager};

pub async fn execute_with<F, Fut>(
    client: &Client,
    box_name: &str,
    user_name: &str,
    f: F,
) -> Result<UserSessionResponse>
where
    F: for<'a> FnOnce(SessionManager, SessionContextSpec<'a>) -> Fut,
    Fut: Future<Output = Result<()>>,
{
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

    // check the box state
    let api = Api::<BoxCrd>::all(client.clone());
    match api.get_opt(box_name).await? {
        Some(_) => {}
        None => return Ok(UserSessionResponse::BoxNotFound),
    }

    // get the box as a node
    let api = Api::<Node>::all(client.clone());
    let node = match api.get_opt(box_name).await? {
        Some(node) => node,
        None => return Ok(UserSessionResponse::BoxNotInCluster),
    };
    let node_capacity = node
        .status
        .as_ref()
        .and_then(|status| status.capacity.as_ref());

    // parse box quota
    let box_quota = {
        let api = Api::<UserBoxBindingCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .filter(|item| item.spec.user == user_name)
            .map(|_| None)
            .next()
    };

    let box_quota = match box_quota {
        Some(_) => box_quota,
        None => {
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

            let api = Api::<UserBoxQuotaBindingCrd>::all(client.clone());
            let lp = ListParams::default();
            api.list(&lp)
                .await?
                .items
                .into_iter()
                .filter(|item| {
                    item.spec
                        .expired_timestamp
                        .as_ref()
                        .map(|timestamp| timestamp < &now)
                        .unwrap_or(true)
                })
                .filter(|item| item.spec.user == user_name)
                .filter_map(|item| quotas.get(&item.spec.quota).cloned())
                .filter(|item| crate::node_selector::is_affordable(node_capacity, &item.compute))
                .map(Some)
                .next()
        }
    };

    // parse user role
    let role = {
        // get available quotas
        let roles = {
            let api = Api::<UserRoleCrd>::all(client.clone());
            let lp = ListParams::default();
            api.list(&lp)
                .await?
                .items
                .into_iter()
                .map(|item| (item.name_any(), item.spec))
                .collect::<BTreeMap<_, _>>()
        };

        let api = Api::<UserRoleBindingCrd>::all(client.clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .filter(|item| item.spec.user == user_name)
            .filter_map(|item| roles.get(&item.spec.role).cloned())
            .sum()
    };

    match box_quota {
        // Login Successed!
        Some(box_quota) => {
            let session_manager = SessionManager::try_new(client.clone()).await?;
            let spec = SessionContextSpec {
                box_quota: box_quota.as_ref(),
                node: &node,
                role: Some(&role),
                user_name: &user.name_any(),
            };

            f(session_manager, spec)
                .await
                .map(|()| UserSessionResponse::Accept {
                    box_quota,
                    user: user.spec,
                })
        }
        None => {
            warn!("[{now}] login denied: {user_name:?} => {box_name:?}");
            Ok(UserSessionResponse::Deny { user: user.spec })
        }
    }
}
