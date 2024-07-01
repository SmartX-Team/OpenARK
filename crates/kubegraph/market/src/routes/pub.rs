use actix_web::{
    delete, get, put,
    web::{Data, Json, Path, Query},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{product::ProductSpec, r#pub::PubSpec, BaseModel, Page};
use tracing::{instrument, Level};

use crate::db::Database;

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod/{prod_id}/pub")]
pub async fn list(
    db: Data<Database>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    page: Query<Page>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.list_pub_ids(prod_id, page.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod/{prod_id}/pub/{pub_id}")]
pub async fn get(
    db: Data<Database>,
    path: Path<(<ProductSpec as BaseModel>::Id, <PubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (_prod_id, pub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.get_pub(pub_id).await))
}

#[instrument(level = Level::INFO, skip(db, spec))]
#[put("/prod/{prod_id}/pub")]
pub async fn put(
    db: Data<Database>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    spec: Json<PubSpec>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.insert_pub(prod_id, spec.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[delete("/prod/{prod_id}/pub/{pub_id}")]
pub async fn delete(
    db: Data<Database>,
    path: Path<(<ProductSpec as BaseModel>::Id, <PubSpec as BaseModel>::Id)>,
) -> impl Responder {
    let (_prod_id, pub_id) = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.remove_pub(pub_id).await))
}
