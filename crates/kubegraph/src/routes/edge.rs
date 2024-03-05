use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::graph::{NetworkNodeKey, NetworkValue};
use kubegraph_client::NetworkGraphClient;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph))]
#[get("/{kind}/{from_namespace}/{from_name}/{to_namespace}/{to_name}")]
pub async fn get(
    graph: Data<NetworkGraphClient>,
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

    HttpResponse::Ok().json(Result::from(
        graph
            .get_edge(&(from, to))
            .await
            .map(|node| node.map(NetworkValue::into_json)),
    ))
}
