use std::{collections::BTreeMap, net::SocketAddr};

use actix_web::{get, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use base64::Engine;
use ipis::{
    core::{
        anyhow::{bail, Error, Result},
        chrono::Utc,
    },
    env::infer,
    log::{error, info, warn},
    logger,
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

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("vine-bastion")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/auth")]
async fn get_auth(request: HttpRequest, client: Data<Client>) -> impl Responder {
    async fn try_handle(request: HttpRequest, client: Data<Client>) -> Result<UserAuthResponse> {
        // get current time
        let now = Utc::now();

        // parse the Authorization token
        let payload: UserAuthPayload = match request.headers().get("Authorization") {
            Some(token) => match token.to_str().map_err(Error::from).and_then(|token| {
                match token
                    .strip_prefix("Bearer ")
                    .and_then(|token| token.split('.').nth(1))
                {
                    Some(payload) => ::base64::engine::general_purpose::STANDARD
                        .decode(payload)
                        .map_err(Into::into)
                        .and_then(|payload| ::serde_json::from_slice(&payload).map_err(Into::into)),
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
                    let name = item.name_any();
                    Some(UserBoxBindingSpec {
                        user: item.spec.user,
                        r#box: boxes.get(&name)?.clone(),
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
                    let name = item.name_any();
                    Some(UserBoxQuotaBindingSpec {
                        user: item.spec.user,
                        quota: quotas.get(&name)?.clone(),
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

    match try_handle(request, client).await {
        Ok(response) if matches!(response, UserAuthResponse::Accept { .. }) => {
            HttpResponse::Ok().json(response)
        }
        Ok(response) => HttpResponse::Forbidden().json(response),
        Err(e) => {
            error!("failed to register a client: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> Result<()> {
        // Initialize kubernetes client
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let client = Data::new(Client::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::clone(&client))
                .service(index)
                .service(health)
                .service(get_auth)
        })
        .bind(addr)
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
        .run()
        .await
        .map_err(Into::into)
    }

    logger::init_once();
    try_main().await.expect("running a server")
}
