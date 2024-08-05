use actix_web::{post, web::Json, HttpResponse, Responder};
use ark_core::result::Result;
use kubegraph_api::market::transaction::TransactionReceipt;
use tracing::{info, instrument, Level};

#[instrument(level = Level::INFO, skip())]
#[post("/")]
pub async fn post(receipt: Json<TransactionReceipt>) -> impl Responder {
    info!("{receipt:#?}");
    HttpResponse::Ok().json(Result::Ok(()))
}
