use std::{collections::BTreeMap, future::Future};

use anyhow::Result;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Node;
use kube::{api::ListParams, Api, Client, ResourceExt};
use tracing::{instrument, warn, Level};
use vine_api::{
    user::UserCrd,
    user_auth::{UserAuthError, UserSessionError, UserSessionResponse},
    user_box_binding::UserBoxBindingCrd,
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::UserBoxQuotaBindingCrd,
    user_role::UserRoleCrd,
    user_role_binding::UserRoleBindingCrd,
};
use vine_session::{is_persistent, AllocationState, SessionContextSpecOwned, SessionManager};

#[instrument(level = Level::INFO, skip(client, f), err(Display))]
pub async fn execute_with<'f, Fut>(
    client: &Client,
    box_name: &str,
    user_name: &str,
    check_resources: bool,
    f: impl FnOnce(SessionManager, SessionContextSpecOwned) -> Fut,
) -> Result<UserSessionResponse>
where
    Fut: 'f + Future<Output = Result<()>>,
{
    // get current time
    let now: DateTime<Utc> = Utc::now();

    // get the user CR
    let user = {
        let api = Api::<UserCrd>::all(client.clone());
        match api.get_opt(user_name).await? {
            Some(user) => match assert_allocable(&user, box_name, user_name, now) {
                Some(error) => return Ok(error),
                None => user,
            },
            None => {
                warn!("[{now}] failed to find an user: {user_name:?} => {box_name:?}");
                return Ok(UserAuthError::UserNotRegistered.into());
            }
        }
    };

    // check the box state
    {
        let api = Api::<Node>::all(client.clone());
        match api.get_opt(box_name).await? {
            Some(_) => {}
            None => return Ok(UserSessionResponse::Error(UserSessionError::NodeNotFound)),
        }
    }

    // get the box as a node
    let node = {
        let api = Api::<Node>::all(client.clone());
        match api.get_opt(box_name).await? {
            Some(node) => match assert_allocable(&node, box_name, user_name, now) {
                Some(error) => return Ok(error),
                None => node,
            },
            None => {
                return Ok(UserSessionResponse::Error(
                    UserSessionError::NodeNotInCluster,
                ))
            }
        }
    };

    // get available resources
    let available_resources = node.status.as_ref().and_then(|status| {
        if check_resources {
            status.allocatable.as_ref()
        } else {
            status.capacity.as_ref()
        }
    });

    // check the box bindings
    {
        let api = Api::<UserBoxBindingCrd>::all(client.clone());
        let lp = ListParams::default();
        let bindings: Vec<_> = api
            .list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| item.spec.r#box == box_name)
            .filter(|item| {
                item.spec
                    .expired_timestamp
                    .as_ref()
                    .map(|timestamp| timestamp < &now)
                    .unwrap_or(true)
            })
            .collect();

        if !bindings.is_empty() && bindings.iter().all(|item| item.spec.user != user_name) {
            return Ok(UserSessionResponse::Error(UserSessionError::NodeReserved));
        }
    }

    let box_quota = {
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
            .find(|item| crate::node_selector::is_affordable(available_resources, &item.compute))
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

            let persistence = is_persistent(&node);
            let spec = SessionContextSpecOwned {
                box_quota: Some(box_quota.clone()),
                node,
                persistence,
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
            warn!("[{now}] quota mismatched: {user_name:?} => {box_name:?}");
            Ok(UserSessionResponse::Error(
                UserSessionError::QuotaMismatched,
            ))
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
