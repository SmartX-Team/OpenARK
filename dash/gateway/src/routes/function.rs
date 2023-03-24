use actix_web::{get, web::Data, HttpResponse, Responder};
use dash_actor::{client::SessionResult, storage::KubernetesStorageClient};
use dash_api::kube::Client;

#[get("/function/{name}")]
pub async fn get(kube: Data<Client>, name: String) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_function(&name).await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/function/")]
pub async fn get_list(kube: Data<Client>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_function_all().await;
    HttpResponse::from(SessionResult::from(result))
}
