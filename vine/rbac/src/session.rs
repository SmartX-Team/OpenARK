use std::{collections::BTreeMap, future::Future};

use anyhow::Result;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Node;
use kube::{api::ListParams, Api, Client, ResourceExt};
use log::warn;
use vine_api::{
    user::UserCrd,
    user_auth::{UserAuthError, UserSessionError, UserSessionResponse},
    user_box_binding::UserBoxBindingCrd,
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::UserBoxQuotaBindingCrd,
    user_role::UserRoleCrd,
    user_role_binding::UserRoleBindingCrd,
};
use vine_session::{AllocationState, SessionContextSpecOwned, SessionManager};

pub async fn execute_with<'f, Fut>(
    client: &Client,
    box_name: &str,
    user_name: &str,
    f: impl FnOnce(SessionManager, SessionContextSpecOwned) -> Fut,
) -> Result<UserSessionResponse>
where
    Fut: 'f + Future<Output = Result<()>>,
{
    // get current time
    let now: DateTime<Utc> = Utc::now();

    // get the user CR
    let api = Api::<UserCrd>::all(client.clone());
    let user = match api.get_opt(user_name).await? {
        Some(user) => match assert_allocable(&user, box_name, user_name, now) {
            Some(error) => return Ok(error),
            None => user,
        },
        None => {
            warn!("[{now}] failed to find an user: {user_name:?} => {box_name:?}");
            return Ok(UserAuthError::UserNotRegistered.into());
        }
    };

    // check the box state
    let api = Api::<Node>::all(client.clone());
    match api.get_opt(box_name).await? {
        Some(_) => {}
        None => return Ok(UserSessionResponse::Error(UserSessionError::NodeNotFound)),
    }

    // get the box as a node
    let api = Api::<Node>::all(client.clone());
    let node = match api.get_opt(box_name).await? {
        Some(node) => match assert_allocable(&node, box_name, user_name, now) {
            Some(error) => return Ok(error),
            None => node,
        },
        None => {
            return Ok(UserSessionResponse::Error(
                UserSessionError::NodeNotInCluster,
            ))
        }
    };

    // get available resources
    let available_resources = node
        .status
        .as_ref()
        .and_then(|status| status.allocatable.as_ref());

    // parse box quota
    let box_quota = {
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
                .filter(|item| item.spec.user == user_name)
                .filter(|item| {
                    item.spec
                        .expired_timestamp
                        .as_ref()
                        .map(|timestamp| timestamp < &now)
                        .unwrap_or(true)
                })
                .filter_map(|item| quotas.get(&item.spec.quota).cloned())
                .filter(|item| {
                    crate::node_selector::is_affordable(available_resources, &item.compute)
                })
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
            .filter(|item| item.spec.user == user_name)
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .filter_map(|item| roles.get(&item.spec.role).cloned())
            .sum()
    };

    match box_quota {
        // Login Successed!
        Some(box_quota) => {
            let namespace = UserCrd::user_namespace_with(user_name);
            let session_manager =
                SessionManager::try_new(namespace.clone(), client.clone()).await?;
            let spec = SessionContextSpecOwned {
                box_quota: box_quota.clone(),
                node,
                role: Some(role),
                user_name: user_name.into(),
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
            Ok(UserSessionResponse::Error(UserSessionError::Deny {
                user: user.spec,
            }))
        }
    }
}

fn assert_allocable<T>(
    object: &T,
    box_name: &str,
    user_name: &str,
    now: DateTime<Utc>,
) -> Option<UserSessionResponse>
where
    T: ResourceExt,
{
    match ::vine_session::is_allocable(object.labels(), box_name, user_name) {
        AllocationState::AllocatedByMyself | AllocationState::NotAllocated => None,
        AllocationState::AllocatedByOtherNode { node_name } => {
            warn!("[{now}] the user is already allocated to {node_name:?}: {user_name:?}");
            Some(UserSessionResponse::Error(
                UserSessionError::AlreadyLoggedInByNode {
                    node_name: node_name.into(),
                },
            ))
        }
        AllocationState::AllocatedByOtherUser { user_name } => {
            warn!("[{now}] the node is already allocated by {user_name:?}: {user_name:?}");
            Some(UserSessionResponse::Error(
                UserSessionError::AlreadyLoggedInByUser {
                    user_name: user_name.into(),
                },
            ))
        }
    }
}
