use actix_web::{
    delete, get, put,
    web::{Data, Json, Path, Query},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{product::ProductSpec, sub::SubSpec, BaseModel, Page};
use tracing::{instrument, Level};

use crate::db::Database;

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod/{prod_id}/sub")]
pub async fn list(
    db: Data<Database>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    page: Query<Page>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.list_sub_ids(prod_id, page.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod/{prod_id}/sub/{sub_id}")]
pub async fn get(
    db: Data<Database>,
    path: Path<(<ProductSpec as BaseModel>::Id, <SubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (_prod_id, sub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.get_sub(sub_id).await))
}

#[instrument(level = Level::INFO, skip(db, spec))]
#[put("/prod/{prod_id}/sub")]
pub async fn put(
    db: Data<Database>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    spec: Json<SubSpec>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.insert_sub(prod_id, spec.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[delete("/prod/{prod_id}/sub/{sub_id}")]
pub async fn delete(
    db: Data<Database>,
    path: Path<(<ProductSpec as BaseModel>::Id, <SubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (_prod_id, sub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.remove_sub(sub_id).await))
}
