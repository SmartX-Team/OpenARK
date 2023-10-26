use std::collections::BTreeMap;

use actix_web::{
    delete, get, post,
    web::{Data, Json, Path},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_provider::input::Name;
use dash_provider_client::DashProviderClient;
use kube::Client;
use serde_json::Value;
use vine_api::user_session::UserSessionRef;
use vine_rbac::auth::AuthUserSession;

#[delete("/task/{task_name}/job/{job_name}/")]
pub async fn delete(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (task_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.delete(&task_name.0, &job_name.0).await;
    HttpResponse::from(Result::from(result))
}

#[get("/task/{task_name}/job/{job_name}/")]
pub async fn get(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (task_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.get(&task_name.0, &job_name.0).await;
    HttpResponse::from(Result::from(result))
}

#[get("/job/")]
pub async fn get_list(request: HttpRequest, kube: Data<Client>) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.get_list().await;
    HttpResponse::from(Result::from(result))
}

#[get("/task/{task_name}/job/")]
pub async fn get_list_with_task_name(
    request: HttpRequest,
    kube: Data<Client>,
    task_name: Path<Name>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.get_list_with_task_name(&task_name.0).await;
    HttpResponse::from(Result::from(result))
}

#[get("/task/{task_name}/job/{job_name}/logs/")]
pub async fn get_stream_logs(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (task_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    match client
        .get_stream_logs_as_bytes(&task_name.0, &job_name.0)
        .await
    {
        Ok(stream) => HttpResponse::Ok().streaming(stream),
        Err(error) => HttpResponse::Forbidden().body(error.to_string()),
    }
}

#[post("/task/{task_name}/job/")]
pub async fn post(
    request: HttpRequest,
    kube: Data<Client>,
    task_name: Path<Name>,
    value: Json<BTreeMap<String, Value>>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.create(&task_name.0, value.0).await;
    HttpResponse::from(Result::from(result))
}

#[post("/task/{task_name}/job/{job_name}/restart/")]
pub async fn post_restart(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (task_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.restart(&task_name.0, &job_name.0).await;
    HttpResponse::from(Result::from(result))
}
