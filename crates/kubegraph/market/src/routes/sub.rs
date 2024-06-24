use actix_web::{
    delete, get, put,
    web::{Data, Json, Path, Query},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{product::ProductSpec, sub::SubSpec, BaseModel, Page};
use tracing::{instrument, Level};

use crate::agent::Agent;

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod/{prod_id}/sub")]
pub async fn list(
    agent: Data<Agent>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    page: Query<Page>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.list_sub(prod_id, page.0).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[get("/prod/{prod_id}/sub/{sub_id}")]
pub async fn get(
    agent: Data<Agent>,
    path: Path<(<ProductSpec as BaseModel>::Id, <SubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (prod_id, sub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.get_sub(prod_id, sub_id).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[put("/prod/{prod_id}/sub")]
pub async fn put(
    agent: Data<Agent>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    spec: Json<SubSpec>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.put_sub(prod_id, spec.0).await))
}

#[instrument(level = Level::INFO, skip(agent))]
#[delete("/prod/{prod_id}/sub/{sub_id}")]
pub async fn delete(
    agent: Data<Agent>,
    path: Path<(<ProductSpec as BaseModel>::Id, <SubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (prod_id, sub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(agent.delete_sub(prod_id, sub_id).await))
}
