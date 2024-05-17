use actix_web::{get, web::Data, HttpResponse, Responder};
use ark_core::result::Result;
use kubegraph_api::db::NetworkGraphDB;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(db))]
#[get("/")]
pub async fn get(db: Data<crate::db::NetworkGraphDB>) -> impl Responder {
    HttpResponse::Ok().json(Result::Ok(db.get_entries(None).await))
}
