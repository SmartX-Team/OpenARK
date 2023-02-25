use std::{net::SocketAddr, ops::Deref};

use actix_web::{get, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use ipis::{core::anyhow::Result, env::infer, log::error, logger};
use vine_api::{kube::Client, user_auth::UserLoginResponse};

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/")]
async fn get_login(
    request: HttpRequest,
    box_name: Data<BoxName>,
    client: Data<Client>,
) -> impl Responder {
    match ::vine_rbac::login::execute(request, &**box_name, client).await {
        Ok(response) if matches!(response, UserLoginResponse::Accept { .. }) => {
            HttpResponse::Ok().json(response)
        }
        Ok(response) => HttpResponse::Forbidden().json(response),
        Err(e) => {
            error!("failed to auth: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

struct BoxName(String);

impl Deref for BoxName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> Result<()> {
        // Initialize kubernetes client
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let box_name = Data::new(BoxName(infer("BOX_NAME")?));
        let client = Data::new(Client::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::clone(&box_name))
                .app_data(Data::clone(&client))
                .service(health)
                .service(get_login)
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
