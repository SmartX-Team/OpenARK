use actix_web::{
    get, post,
    web::{Data, Json, Path},
    HttpResponse, Responder,
};
use dash_provider::{
    client::{FunctionSession, SessionContextMetadata, SessionResult},
    input::{InputField, Name},
    storage::KubernetesStorageClient,
};
use kube::Client;
use serde_json::Value;

#[get("/function/{name}/")]
pub async fn get(kube: Data<Client>, name: Path<Name>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_function(&name.0).await;
    HttpResponse::from(SessionResult::from(result))
}

#[get("/function/")]
pub async fn get_list(kube: Data<Client>) -> impl Responder {
    let client = KubernetesStorageClient { kube: &kube };
    let result = client.load_function_all().await;
    HttpResponse::from(SessionResult::from(result))
}

#[post("/function/{name}/")]
pub async fn post(kube: Data<Client>, name: Path<Name>, value: Json<Value>) -> impl Responder {
    let kube = kube.as_ref().clone();
    let metadata = SessionContextMetadata {
        name: name.into_inner().0,
        namespace: "vine".to_string(), // TODO: to be implemented
    };
    let inputs = vec![InputField {
        name: "/".to_string(),
        value: value.0,
    }];

    let result = FunctionSession::create_raw(kube, &metadata, inputs).await;
    HttpResponse::from(result)
}
