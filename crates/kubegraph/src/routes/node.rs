use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::graph::{NetworkNode, NetworkNodeKey};
use kubegraph_client::NetworkGraphClient;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph))]
#[get("/{kind}/{namespace}/{name}")]
pub async fn get(
    graph: Data<NetworkGraphClient>,
    path: Path<(String, String, String)>,
) -> impl Responder {
    let (kind, namespace, name) = path.into_inner();
    let key = NetworkNodeKey {
        kind,
        name: if name == "_" { None } else { Some(name) },
        namespace,
    };

    HttpResponse::Ok().json(Result::from(
        graph
            .get_node(&key)
            .await
            .map(|node| node.map(NetworkNode::into_json)),
    ))
}
