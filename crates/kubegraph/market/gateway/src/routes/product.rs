use actix_web::{
    delete, get, put,
    web::{Data, Json, Path, Query},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{product::ProductSpec, BaseModel, Page};
use tracing::{instrument, Level};

use crate::db::Database;

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod")]
pub async fn list(db: Data<Database>, page: Query<Page>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(db.list_product_ids(page.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod/{prod_id}/price")]
pub async fn list_price(
    db: Data<Database>,
    path: Path<<ProductSpec as BaseModel>::Id>,
    page: Query<Page>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.list_price_histogram(prod_id, page.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[get("/prod/{prod_id}")]
pub async fn get(db: Data<Database>, path: Path<<ProductSpec as BaseModel>::Id>) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.get_product(prod_id).await))
}

#[instrument(level = Level::INFO, skip(db, spec))]
#[put("/prod")]
pub async fn put(db: Data<Database>, spec: Json<ProductSpec>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(db.insert_product(spec.0).await))
}

#[instrument(level = Level::INFO, skip(db))]
#[delete("/prod/{prod_id}")]
pub async fn delete(
    db: Data<Database>,
    path: Path<<ProductSpec as BaseModel>::Id>,
) -> impl Responder {
    let prod_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.remove_product(prod_id).await))
}
