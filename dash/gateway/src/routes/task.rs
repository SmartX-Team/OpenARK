use actix_web::{
    get,
    web::{Data, Path},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_provider::{input::Name, storage::KubernetesStorageClient};
use kube::Client;

#[get("/task/{name}/")]
pub async fn get(request: HttpRequest, kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let kube = kube.as_ref();
    let namespace = match ::vine_rbac::auth::get_user_namespace(kube, &request).await {
        Ok(namespace) => namespace,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = KubernetesStorageClient {
        namespace: &namespace,
        kube,
    };
    let result = client.load_task(&name.0).await;
    HttpResponse::from(Result::from(result))
}

#[get("/task/")]
pub async fn get_list(request: HttpRequest, kube: Data<Client>) -> impl Responder {
    let kube = kube.as_ref();
    let namespace = match ::vine_rbac::auth::get_user_namespace(kube, &request).await {
        Ok(namespace) => namespace,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = KubernetesStorageClient {
        namespace: &namespace,
        kube,
    };
    let result = client.load_task_all().await;
    HttpResponse::from(Result::from(result))
}
