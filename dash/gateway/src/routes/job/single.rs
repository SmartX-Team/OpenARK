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
use vine_rbac::auth::UserSessionRef;

#[delete("/function/{function_name}/job/{job_name}/")]
pub async fn delete(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (function_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.delete(&function_name.0, &job_name.0).await;
    HttpResponse::from(Result::from(result))
}

#[get("/function/{function_name}/job/{job_name}/")]
pub async fn get(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (function_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.get(&function_name.0, &job_name.0).await;
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

#[get("/function/{function_name}/job/")]
pub async fn get_list_with_function_name(
    request: HttpRequest,
    kube: Data<Client>,
    function_name: Path<Name>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.get_list_with_function_name(&function_name.0).await;
    HttpResponse::from(Result::from(result))
}

#[get("/function/{function_name}/job/{job_name}/logs/")]
pub async fn get_stream_logs(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (function_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    match client
        .get_stream_logs_as_bytes(&function_name.0, &job_name.0)
        .await
    {
        Ok(stream) => HttpResponse::Ok().streaming(stream),
        Err(error) => HttpResponse::Forbidden().body(error.to_string()),
    }
}

#[post("/function/{function_name}/job/")]
pub async fn post(
    request: HttpRequest,
    kube: Data<Client>,
    function_name: Path<Name>,
    value: Json<BTreeMap<String, Value>>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.create(&function_name.0, value.0).await;
    HttpResponse::from(Result::from(result))
}

#[post("/function/{function_name}/job/{job_name}/restart/")]
pub async fn post_restart(
    request: HttpRequest,
    kube: Data<Client>,
    path: Path<(Name, Name)>,
) -> impl Responder {
    let (function_name, job_name) = path.into_inner();
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let client = DashProviderClient::new(kube, &session);
    let result = client.restart(&function_name.0, &job_name.0).await;
    HttpResponse::from(Result::from(result))
}
