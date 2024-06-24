use actix_web::{
    delete, get, put,
    web::{Data, Json, Path, Query},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{product::ProductSpec, BaseModel, Page};
use tracing::{instrument, Level};

use crate::agent::Agent;

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod")]
pub async fn list(agent: Data<Agent>, page: Query<Page>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(agent.list_product(page.0).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod/{prod_id}/price")]
pub async fn list_price(
    agent: Data<Agent>,
    path: Path<<ProductSpec as BaseModel>::Id>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.list_price(prod_id).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod/{prod_id}")]
pub async fn get(agent: Data<Agent>, path: Path<<ProductSpec as BaseModel>::Id>) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.get_product(prod_id).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[put("/prod")]
pub async fn put(agent: Data<Agent>, spec: Json<ProductSpec>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(agent.put_product(spec.0).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[delete("/prod/{prod_id}")]
pub async fn delete(
    agent: Data<Agent>,
    path: Path<<ProductSpec as BaseModel>::Id>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.delete_product(prod_id).await))
}
