use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use dash_api::kube::Client;
use dash_provider::{
    client::SessionResult,
    input::Name,
    storage::{KubernetesStorageClient, StorageClient},
};

#[get("/model/{name}/")]
pub async fn get(kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_model(&name.0).await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/model/{name}/function/")]
pub async fn get_function_list(kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_function_all_by_model(&name.0).await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/model/")]
pub async fn get_list(kube: Data<Client>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_model_all().await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/model/{name}/item/{item}/")]
pub async fn get_item(kube: Data<Client>, name: Path<(Name, Name)>) -> impl Responder {
    let client = StorageClient {
        namespace: "vine", // TODO: to be implemented
        kube: &kube,
    };
    let result = client.get_by_model(&name.0 .0, &name.1 .0).await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/model/{name}/item/")]
pub async fn get_item_list(kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let client = StorageClient {
        namespace: "vine", // TODO: to be implemented
        kube: &kube,
    };
    let result = client.list_by_model(&name.0).await;
    HttpResponse::from(SessionResult::from(result))
}
