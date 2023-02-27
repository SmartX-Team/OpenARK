use std::collections::BTreeMap;

use actix_web::{web::Data, HttpRequest};
use base64::Engine;
use ipis::{
    core::{
        anyhow::{bail, Error, Result},
        chrono::Utc,
    },
    log::{info, warn},
};
use kiss_api::{
    k8s_openapi::api::core::v1::Node,
    kube::{api::ListParams, ResourceExt},
    r#box::{BoxCrd, BoxState},
};
use vine_api::{
    kube::{Api, Client},
    user::UserCrd,
    user_auth::{UserAuthPayload, UserLoginResponse},
    user_box_binding::UserBoxBindingCrd,
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::UserBoxQuotaBindingCrd,
};

pub async fn execute(
    request: &HttpRequest,
    box_name: &str,
    client: Data<Client>,
) -> Result<UserLoginResponse> {
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
                None => bail!("the Authorization token is not a Bearer token"),
            }
        }) {
            Ok(payload) => payload,
            Err(e) => {
                warn!("failed to parse the token: {token:?}: {e}");
                return Ok(UserLoginResponse::AuthorizationTokenMalformed);
            }
        },
        None => {
            warn!("failed to get the token: Authorization");
            return Ok(UserLoginResponse::AuthorizationTokenNotFound);
        }
    };

    // get the user primary key
    let primary_key = match payload.primary_key() {
        Ok(key) => key,
        Err(e) => {
            warn!("failed to parse the user's primary key: {payload:?}: {e}");
            return Ok(UserLoginResponse::PrimaryKeyMalformed);
        }
    };

    // get the user CR
    let api = Api::<UserCrd>::all((**client).clone());
    let user = match api.get_opt(&primary_key).await? {
        Some(user) => user.spec,
        None => {
            warn!("failed to find an user: {primary_key:?}");
            return Ok(UserLoginResponse::UserNotRegistered);
        }
    };

    // check the box state
    let api = Api::<BoxCrd>::all((**client).clone());
    match api.get_opt(box_name).await? {
        Some(r#box)
            if r#box.status.as_ref().map(|status| status.state) == Some(BoxState::Running) => {}
        Some(_) => return Ok(UserLoginResponse::BoxNotRunning),
        None => return Ok(UserLoginResponse::BoxNotFound),
    }

    // get the box as a node
    let api = Api::<Node>::all((**client).clone());
    let node = match api.get_opt(box_name).await? {
        Some(node) => node,
        None => return Ok(UserLoginResponse::BoxNotInCluster),
    };
    let node_capacity = node
        .status
        .as_ref()
        .and_then(|status| status.capacity.as_ref());

    let box_quota = {
        let api = Api::<UserBoxBindingCrd>::all((**client).clone());
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
            .filter(|item| item.spec.user == primary_key)
            .map(|_| None)
            .next()
    };

    let box_quota = match box_quota {
        Some(_) => box_quota,
        None => {
            // get available quotas
            let quotas = {
                let api = Api::<UserBoxQuotaCrd>::all((**client).clone());
                let lp = ListParams::default();
                api.list(&lp)
                    .await?
                    .items
                    .into_iter()
                    .map(|item| (item.name_any(), item.spec))
                    .collect::<BTreeMap<_, _>>()
            };

            let api = Api::<UserBoxQuotaBindingCrd>::all((**client).clone());
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
                .filter(|item| item.spec.user == primary_key)
                .filter_map(|item| quotas.get(&item.spec.quota).cloned())
                .filter(|item| crate::node_selector::is_affordable(node_capacity, &item.resources))
                .map(Some)
                .next()
        }
    };

    match box_quota {
        // Login Successed!
        Some(box_quota) => {
            info!("login accepted: {primary_key:?}");
            Ok(UserLoginResponse::Accept { box_quota, user })
        }
        None => {
            warn!("login denied: {primary_key:?}");
            Ok(UserLoginResponse::Deny { user })
        }
    }
}
