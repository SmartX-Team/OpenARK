use actix_web::{get, web::Data, HttpResponse, Responder};
use ark_core::result::Result;
use futures::{stream::FuturesUnordered, TryFutureExt, TryStreamExt};
use kubegraph_api::graph::NetworkGraphDB;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph_db))]
#[get("/")]
pub async fn get(graph_db: Data<Box<dyn Send + NetworkGraphDB>>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(
        graph_db
            .list(None)
            .and_then(|graph| {
                graph
                    .into_iter()
                    .map(|graph| graph.collect())
                    .collect::<FuturesUnordered<_>>()
                    .map_ok(|graph| graph.drop_null_columns())
                    .try_collect::<Vec<_>>()
            })
            .await,
    ))
}
