use actix_web::{
    delete, get, put,
    web::{Data, Json, Path, Query},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{product::ProductSpec, r#pub::PubSpec, BaseModel, Page};
use tracing::{instrument, Level};

use crate::agent::Agent;

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod/{prod_id}/pub")]
pub async fn list(
    agent: Data<Agent>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    page: Query<Page>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.list_pub(prod_id, page.0).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod/{prod_id}/pub/{pub_id}")]
pub async fn get(
    agent: Data<Agent>,
    path: Path<(<ProductSpec as BaseModel>::Id, <PubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (prod_id, pub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.get_pub(prod_id, pub_id).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[put("/prod/{prod_id}/pub")]
pub async fn put(
    agent: Data<Agent>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    spec: Json<PubSpec>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.put_pub(prod_id, spec.0).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[delete("/prod/{prod_id}/pub/{pub_id}")]
pub async fn delete(
    agent: Data<Agent>,
    path: Path<(<ProductSpec as BaseModel>::Id, <PubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (prod_id, pub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.delete_pub(prod_id, pub_id).await))
}
