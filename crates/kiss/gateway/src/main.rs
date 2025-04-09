use std::net::SocketAddr;

use actix_web::{
    get, middleware, post,
    web::{Data, Json, Query},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::{bail, Result};
use ark_core::{env::infer, tracer};
use chrono::Utc;
use kiss_api::r#box::{
    request::{BoxCommissionQuery, BoxNewQuery},
    BoxAccessSpec, BoxCrd, BoxSpec, BoxState, BoxStatus,
};
use kube::{
    api::{Patch, PatchParams, PostParams},
    core::ObjectMeta,
    Api, Client, CustomResourceExt,
};
use opentelemetry::global;
use serde_json::json;
use tracing::{instrument, warn, Level};

#[instrument(level = Level::INFO)]
#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("kiss-gateway")
}

#[instrument(level = Level::INFO)]
#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[instrument(level = Level::INFO, skip(client))]
#[get("/new")]
async fn get_new(client: Data<Client>, Query(query): Query<BoxNewQuery>) -> impl Responder {
    async fn try_handle(client: Data<Client>, query: BoxNewQuery) -> Result<()> {
        let api = Api::<BoxCrd>::all((**client).clone());

        let name = query.machine.uuid.to_string();

        match api.get_opt(&name).await? {
            Some(r#box) => {
                let crd = BoxCrd::api_resource();
                let patch = Patch::Merge(json!({
                    "apiVersion": crd.api_version,
                    "kind": crd.kind,
                    "status": BoxStatus {
                        access: BoxAccessSpec {
                            primary: Some(query.access_primary.try_into()?),
                        },
                        state: BoxState::New,
                        bind_group: r#box.status.as_ref().and_then(|status| status.bind_group.as_ref()).cloned(),
                        last_updated: Utc::now(),
                    },
                }));
                let pp = PatchParams::apply("kiss-gateway");
                api.patch_status(&name, &pp, &patch).await?;
            }
            None => {
                let data = BoxCrd {
                    metadata: ObjectMeta {
                        name: Some(name.clone()),
                        ..Default::default()
                    },
                    spec: BoxSpec {
                        group: Default::default(),
                        machine: query.machine,
                        power: None,
                        rack: None,
                    },
                    status: None,
                };
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some("kiss-gateway".into()),
                };
                api.create(&pp, &data).await?;

                let crd = BoxCrd::api_resource();
                let patch = Patch::Merge(json!({
                    "apiVersion": crd.api_version,
                    "kind": crd.kind,
                    "status": BoxStatus {
                        access: BoxAccessSpec {
                            primary: Some(query.access_primary.try_into()?),
                        },
                        state: BoxState::New,
                        bind_group: None,
                        last_updated: Utc::now(),
                    },
                }));
                let pp = PatchParams::apply("kiss-gateway");
                api.patch_status(&name, &pp, &patch).await?;
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

#[instrument(level = Level::INFO, skip(client))]
#[post("/commission")]
async fn post_commission(
    client: Data<Client>,
    Json(query): Json<BoxCommissionQuery>,
) -> impl Responder {
    async fn try_handle(client: Data<Client>, query: BoxCommissionQuery) -> Result<()> {
        let api = Api::<BoxCrd>::all((**client).clone());

        let name = query.machine.uuid.to_string();

        match api.get_opt(&name).await? {
            Some(r#box) => {
                let crd = BoxCrd::api_resource();
                let patch = Patch::Merge(json!({
                    "apiVersion": crd.api_version,
                    "kind": crd.kind,
                    "spec": BoxSpec {
                        group: r#box.spec.group,
                        machine: query.machine,
                        power: query.power,
                        rack: r#box.spec.rack,
                    },
                    "status": BoxStatus {
                        access: query.access.try_into()?,
                        state: BoxState::Ready,
                        bind_group: if query.reset {
                            None
                        } else {
                            r#box
                                .status
                                .as_ref()
                                .and_then(|status| status.bind_group.as_ref())
                                .cloned()
                        },
                        last_updated: Utc::now(),
                    },
                }));
                let pp = PatchParams::apply("kiss-gateway");
                api.patch(&name, &pp, &patch).await?;
                api.patch_status(&name, &pp, &patch).await?;
            }
            None => bail!("no such box: {name}"),
        }
        Ok(())
    }

    match try_handle(client, query).await {
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
        let client = Data::new(Client::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            let app = App::new().app_data(Data::clone(&client));
            let app = app
                .service(index)
                .service(health)
                .service(get_new)
                .service(post_commission);
            app.wrap(middleware::NormalizePath::new(
                middleware::TrailingSlash::Trim,
            ))
            .wrap(RequestMetrics::default())
            .wrap(RequestTracing::default())
        })
        .bind(addr)
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
        .run()
        .await
        .map_err(Into::into)
    }

    tracer::init_once();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    try_main().await.expect("running a server");
    global::shutdown_tracer_provider()
}
