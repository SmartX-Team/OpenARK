use std::{net::IpAddr, sync::Arc};

use actix_web::{
    get,
    web::{Data, Query},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use ipis::{
    core::{anyhow::Result, chrono::Utc},
    log::warn,
    logger,
};
use kiss_api::{
    kube::{api::PostParams, core::ObjectMeta, Api, Client},
    r#box::{BoxAccessSpec, BoxCrd, BoxMachineSpec, BoxSpec, BoxState, BoxStatus},
};

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("kiss-monitor")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/new")]
async fn get_new(
    client: Data<Arc<Client>>,
    req: HttpRequest,
    Query(hardware): Query<BoxMachineSpec>,
) -> impl Responder {
    async fn try_handle(
        client: Data<Arc<Client>>,
        address: IpAddr,
        machine: BoxMachineSpec,
    ) -> Result<()> {
        let api = Api::<BoxCrd>::all((***client).clone());

        let name = machine.uuid.to_string();
        let data = BoxCrd {
            metadata: ObjectMeta {
                name: Some(name.clone()),
                ..Default::default()
            },
            spec: BoxSpec {
                access: BoxAccessSpec { address },
                machine,
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

        match api.get(&name).await {
            Ok(_) => api.replace(&name, &pp, &data).await.map(|_| ())?,
            Err(_) => api.create(&pp, &data).await.map(|_| ())?,
        }
        Ok(())
    }

    if let Some(addr) = req.peer_addr() {
        match try_handle(client, addr.ip(), hardware).await {
            Ok(()) => HttpResponse::Ok().json("Ok"),
            Err(e) => {
                warn!("failed to register a client: {e}");
                HttpResponse::Forbidden().json("Err")
            }
        }
    } else {
        HttpResponse::Unauthorized().json("Empty address")
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> Result<()> {
        // Initialize kubernetes client
        let client = Arc::new(Client::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::new(client.clone()))
                .service(index)
                .service(health)
                .service(get_new)
        })
        .bind("0.0.0.0:8089")
        .expect("failed to bind to 0.0.0.0:8089")
        .shutdown_timeout(5)
        .run()
        .await
        .map_err(Into::into)
    }

    logger::init_once();
    try_main().await.expect("running a server")
}
