use actix_web::{
    get,
    web::{Data, Path},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_provider::{input::Name, storage::KubernetesStorageClient};
use kube::Client;
use tracing::{instrument, Level};
use vine_api::user_session::UserSession;
use vine_rbac::auth::AuthUserSession;

#[instrument(level = Level::INFO, skip(request, kube))]
#[get("/task/{name}")]
pub async fn get(request: HttpRequest, kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let kube = kube.as_ref();
    let namespace = match UserSession::from_request(&kube, &request).await {
        Ok(session) => session.namespace,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = KubernetesStorageClient {
        namespace: &namespace,
        kube,
    };
    let result = client.load_task(&name.0).await;
    HttpResponse::from(Result::from(result))
}

#[instrument(level = Level::INFO, skip(request, kube))]
#[get("/task")]
pub async fn get_list(request: HttpRequest, kube: Data<Client>) -> impl Responder {
    let kube = kube.as_ref();
    let namespace = match UserSession::from_request(&kube, &request).await {
        Ok(session) => session.namespace,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = KubernetesStorageClient {
        namespace: &namespace,
        kube,
    };
    let result = client.load_task_all().await;
    HttpResponse::from(Result::from(result))
}
