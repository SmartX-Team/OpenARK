use actix_web::{post, web::Json, HttpResponse, Responder};
use ark_core::result::Result;
use kubegraph_api::market::function::MarketFunctionContext;
use tracing::{info, instrument, Level};

#[instrument(level = Level::INFO, skip())]
#[post("/")]
pub async fn post(ctx: Json<MarketFunctionContext>) -> impl Responder {
    info!("{ctx:#?}");
    HttpResponse::Ok().json(Result::Ok(()))
}
