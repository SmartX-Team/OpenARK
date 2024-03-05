use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use dash_network_api::graph::{ArcNetworkGraph, NetworkNodeKey};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/{kind}/{from_namespace}/{from_name}/{to_namespace}/{to_name}")]
pub async fn get(
    graph: Data<ArcNetworkGraph>,
    path: Path<(String, String, String, String, String)>,
) -> impl Responder {
    let (kind, from_namespace, from_name, to_namespace, to_name) = path.into_inner();
    let from = NetworkNodeKey {
        kind: kind.clone(),
        name: if from_name == "_" {
            None
        } else {
            Some(from_name)
        },
        namespace: from_namespace,
    };
    let to = NetworkNodeKey {
        kind,
        name: if to_name == "_" { None } else { Some(to_name) },
        namespace: to_namespace,
    };

    HttpResponse::Ok().json(
        graph
            .get_edge(&(from, to))
            .await
            .map(|node| node.into_json()),
    )
}
