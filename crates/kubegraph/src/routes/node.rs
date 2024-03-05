use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use dash_network_api::graph::{ArcNetworkGraph, NetworkNodeKey};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/{kind}/{namespace}/{name}")]
pub async fn get(
    graph: Data<ArcNetworkGraph>,
    path: Path<(String, String, String)>,
) -> impl Responder {
    let (kind, namespace, name) = path.into_inner();
    let key = NetworkNodeKey {
        kind,
        name: if name == "_" { None } else { Some(name) },
        namespace,
    };

    HttpResponse::Ok().json(graph.get_node(&key).await.map(|node| node.into_json()))
}
