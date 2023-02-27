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
    kube::{api::ListParams, ResourceExt},
    r#box::{BoxCrd, BoxState},
};
use vine_api::{
    kube::{Api, Client},
    user::UserCrd,
    user_auth::{UserAuthPayload, UserAuthResponse},
    user_box_binding::{UserBoxBindingCrd, UserBoxBindingSpec},
    user_box_quota::UserBoxQuotaCrd,
    user_box_quota_binding::{UserBoxQuotaBindingCrd, UserBoxQuotaBindingSpec},
};

pub async fn execute(request: &HttpRequest, client: Data<Client>) -> Result<UserAuthResponse> {
    // get current time
    let now = Utc::now();

    // parse the Authorization token
    let payload: UserAuthPayload = match request.headers().get("Authorization") {
        Some(token) => match token.to_str().map_err(Error::from).and_then(|token| {
            match token
                .strip_prefix("Bearer ")
                .and_then(|token| token.split('.').nth(1))
            {
                Some(payload) => {
                    let payload = ::base64::engine::general_purpose::STANDARD_NO_PAD
                        .decode(payload)
                        .unwrap();
                    let payload = ::serde_json::from_slice(&payload).unwrap();
                    Ok(payload)
                }
                None => bail!("the Authorization token is not a Bearer token"),
            }
        }) {
            Ok(payload) => payload,
            Err(e) => {
                warn!("failed to parse the token: {token:?}: {e}");
                return Ok(UserAuthResponse::AuthorizationTokenMalformed);
            }
        },
        None => {
            warn!("failed to get the token: Authorization");
            return Ok(UserAuthResponse::AuthorizationTokenNotFound);
        }
    };

    // get the user primary key
    let primary_key = match payload.primary_key() {
        Ok(key) => key,
        Err(e) => {
            warn!("failed to parse the user's primary key: {payload:?}: {e}");
            return Ok(UserAuthResponse::PrimaryKeyMalformed);
        }
    };

    // get the user CR
    let api = Api::<UserCrd>::all((**client).clone());
    let user = match api.get_opt(&primary_key).await? {
        Some(user) => user.spec,
        None => {
            warn!("failed to find an user: {primary_key:?}");
            return Ok(UserAuthResponse::UserNotRegistered);
        }
    };

    // get available boxes
    let boxes = {
        let api = Api::<BoxCrd>::all((**client).clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .filter(|item| {
                item.status.as_ref().map(|status| status.state) == Some(BoxState::Running)
            })
            .map(|item| (item.name_any(), item.spec))
            .collect::<BTreeMap<_, _>>()
    };

    let box_bindings = {
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
        let api = Api::<UserBoxQuotaCrd>::all((**client).clone());
        let lp = ListParams::default();
        api.list(&lp)
            .await?
            .items
            .into_iter()
            .map(|item| (item.name_any(), item.spec))
            .collect::<BTreeMap<_, _>>()
    };

    let box_quota_bindings = {
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
    info!("login accepted: {primary_key:?}");
    Ok(UserAuthResponse::Accept {
        box_bindings,
        box_quota_bindings,
        user,
    })
}
