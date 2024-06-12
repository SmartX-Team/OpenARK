use actix_web::{
    get, post,
    web::{Data, Json, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use futures::{stream::FuturesUnordered, TryFutureExt, TryStreamExt};
use kubegraph_api::{
    frame::DataFrame,
    graph::{Graph, GraphData, GraphFilter, NetworkGraphDB},
};
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

#[instrument(level = Level::INFO, skip(graph_db, graph))]
#[post("/{namespace}")]
pub async fn post(
    namespace: Path<String>,
    graph_db: Data<Box<dyn Send + NetworkGraphDB>>,
    Json(graph): Json<Graph<GraphData<DataFrame>>>,
) -> impl Responder {
    if &namespace.into_inner() != &graph.scope.namespace {
        return HttpResponse::Ok().json(Result::Ok(()));
    }

    HttpResponse::Ok().json(Result::from(graph_db.insert(graph.lazy()).await))
}
