#![deny(
    clippy::all,
    clippy::cargo,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]

use std::{net::SocketAddr, sync::Arc};

use actix_web::{
    get, post,
    web::{Data, Json, Query},
    App, HttpResponse, HttpServer, Responder,
};
use ipis::{
    core::{anyhow::Result, chrono::Utc},
    env::infer,
    log::warn,
    logger,
};
use kiss_api::{
    kube::{
        api::{Patch, PatchParams, PostParams},
        core::ObjectMeta,
        Api, Client, CustomResourceExt,
    },
    r#box::{request::BoxNewQuery, BoxCrd, BoxSpec, BoxState, BoxStatus},
    serde_json::json,
};

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("kiss-gateway")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/new")]
async fn get_new(client: Data<Arc<Client>>, Query(query): Query<BoxNewQuery>) -> impl Responder {
    async fn try_handle(client: Data<Arc<Client>>, query: BoxNewQuery) -> Result<()> {
        let api = Api::<BoxCrd>::all((***client).clone());

        let name = query.machine.uuid.to_string();

        match api.get(&name).await {
            Ok(_) => {
                let crd = BoxCrd::api_resource();
                let patch = Patch::Apply(json!({
                    "apiVersion": crd.api_version,
                    "kind": crd.kind,
                    "status": BoxStatus {
                        state: BoxState::New,
                        last_updated: Utc::now(),
                    },
                }));
                let pp = PatchParams::apply("kiss-controller").force();
                api.patch_status(&name, &pp, &patch).await?;
            }
            Err(_) => {
                let data = BoxCrd {
                    metadata: ObjectMeta {
                        name: Some(name.clone()),
                        ..Default::default()
                    },
                    spec: BoxSpec {
                        access: query.access,
                        machine: query.machine,
                        power: None,
                    },
                    status: Some(BoxStatus {
                        state: BoxState::New,
                        last_updated: Utc::now(),
                    }),
                };
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some("kiss-gateway".into()),
                };
                api.create(&pp, &data).await?;
            }
        }
        Ok(())
    }

    match try_handle(client, query).await {
        Ok(()) => HttpResponse::Ok().json("Ok"),
        Err(e) => {
            warn!("failed to register a client: {e}");
            HttpResponse::Forbidden().json("Err")
        }
    }
}

#[post("/commission")]
async fn get_commission(client: Data<Arc<Client>>, Json(spec): Json<BoxSpec>) -> impl Responder {
    async fn try_handle(client: Data<Arc<Client>>, spec: BoxSpec) -> Result<()> {
        let api = Api::<BoxCrd>::all((***client).clone());

        let name = spec.machine.uuid.to_string();

        match api.get(&name).await {
            Ok(_) => {
                let crd = BoxCrd::api_resource();
                let patch = Patch::Apply(json!({
                    "apiVersion": crd.api_version,
                    "kind": crd.kind,
                    "spec": spec,
                    "status": BoxStatus {
                        state: BoxState::Ready,
                        last_updated: Utc::now(),
                    },
                }));
                let pp = PatchParams::apply("kiss-controller").force();
                api.patch(&name, &pp, &patch).await?;
                api.patch_status(&name, &pp, &patch).await?;
            }
            Err(_) => {
                let data = BoxCrd {
                    metadata: ObjectMeta {
                        name: Some(name.clone()),
                        ..Default::default()
                    },
                    spec,
                    status: Some(BoxStatus {
                        state: BoxState::Ready,
                        last_updated: Utc::now(),
                    }),
                };
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some("kiss-gateway".into()),
                };
                api.create(&pp, &data).await?;
            }
        }
        Ok(())
    }

    match try_handle(client, spec).await {
        Ok(()) => HttpResponse::Ok().json("Ok"),
        Err(e) => {
            warn!("failed to commission a client: {e}");
            HttpResponse::Forbidden().json("Err")
        }
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> Result<()> {
        // Initialize kubernetes client
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let client = Arc::new(Client::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::new(client.clone()))
                .service(index)
                .service(health)
                .service(get_new)
                .service(get_commission)
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
