use std::sync::Arc;

use actix_web::{get, web::Data, HttpResponse, Responder};
use ark_core::result::Result;
use futures::stream::FuturesUnordered;
use kubegraph_api::graph::NetworkGraphDB;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(db))]
#[get("/")]
pub async fn get(graph_db: Data<Arc<dyn NetworkGraphDB>>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(
        graph_db
            .list(None)
            .and_then(|graph| {
                graph
                    .into_iter()
                    .map(|graph| graph.collect())
                    .collect::<FuturesUnordered<_>>()
                    .try_collect()
            })
            .await,
    ))
}
