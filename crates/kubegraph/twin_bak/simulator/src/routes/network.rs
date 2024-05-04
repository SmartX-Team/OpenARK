use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use kubegraph_api::{db::NetworkGraphDB, graph::NetworkEntryKeyFilter};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph))]
#[get("/")]
pub async fn get(graph: Data<crate::DefaultNetworkGraphDB>) -> impl Responder {
    let filter = None;
    HttpResponse::Ok().json(Result::Ok(graph.get_entries(filter).await))
}

#[instrument(level = Level::INFO, skip(graph))]
#[get("/{kind}")]
pub async fn get_kind(
    graph: Data<crate::DefaultNetworkGraphDB>,
    path: Path<(String,)>,
) -> impl Responder {
    let (kind,) = path.into_inner();
    let filter = Some(NetworkEntryKeyFilter {
        kind: Some(kind),
        namespace: None,
    });

    HttpResponse::Ok().json(Result::Ok(graph.get_entries(filter.as_ref()).await))
}

#[instrument(level = Level::INFO, skip(graph))]
#[get("/{kind}/{namespace}")]
pub async fn get_kind_namespace(
    graph: Data<crate::DefaultNetworkGraphDB>,
    path: Path<(String, String)>,
) -> impl Responder {
    let (kind, namespace) = path.into_inner();
    let filter = Some(NetworkEntryKeyFilter {
        kind: Some(kind),
        namespace: Some(namespace),
    });

    HttpResponse::Ok().json(Result::Ok(graph.get_entries(filter.as_ref()).await))
}
