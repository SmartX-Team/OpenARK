use actix_web::{
    get,
    web::{Data, Path},
    HttpResponse, Responder,
};
use dash_actor::{client::SessionResult, input::Name, storage::KubernetesStorageClient};
use dash_api::kube::Client;

#[get("/model/{name}/")]
pub async fn get(kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_model(&name.0).await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/model/")]
pub async fn get_list(kube: Data<Client>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_model_all().await;
    HttpResponse::from(SessionResult::from(result))
}
