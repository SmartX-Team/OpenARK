use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use futures::{stream::FuturesUnordered, TryFutureExt, TryStreamExt};
use kubegraph_api::graph::{GraphFilter, NetworkGraphDB};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph_db))]
#[get("/{namespace}")]
pub async fn get(
    namespace: Path<String>,
    graph_db: Data<Box<dyn Send + NetworkGraphDB>>,
) -> impl Responder {
    let filter = GraphFilter::all(namespace.into_inner());

    HttpResponse::Ok().json(Result::from(
        graph_db
            .list(&filter)
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
