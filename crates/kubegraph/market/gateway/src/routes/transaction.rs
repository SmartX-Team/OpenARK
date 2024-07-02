use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::market::{transaction::TransactionSpec, BaseModel};
use tracing::{instrument, Level};

use crate::db::Database;

#[instrument(level = Level::INFO, skip(db))]
#[get("/txn/{txn_id}")]
pub async fn get(
    db: Data<Database>,
    path: Path<<TransactionSpec as BaseModel>::Id>,
) -> impl Responder {
    let txn_id = path.into_inner();
    HttpResponse::Ok().json(Result::from(db.get_transaction(txn_id).await))
}
