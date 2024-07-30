use std::collections::BTreeMap;

use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_api::job::DashJobCrd;
use dash_provider_api::job::Payload;
use dash_provider_client::DashProviderClient;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::Client;
use serde_json::Value;
use tracing::{instrument, Level};
use vine_api::user_session::UserSession;
use vine_rbac::auth::AuthUserSession;

#[instrument(level = Level::INFO, skip(request, kube))]
#[post("/batch/job")]
pub async fn post(
    request: HttpRequest,
    kube: Data<Client>,
    values: Json<Vec<Payload<BTreeMap<String, Value>>>>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSession::from_request(&kube, &request).await {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let result: ::core::result::Result<Vec<DashJobCrd>, _> = values
        .0
        .into_iter()
        .map(
            |Payload {
                 task_name,
                 namespace,
                 value,
             }| {
                let kube = kube.clone();
                let session = session.clone();
                async move {
                    let session = session.namespaced(namespace).await?;
                    let client = DashProviderClient::new(kube, &session);
                    client.create(&task_name, value).await
                }
            },
        )
        .collect::<FuturesUnordered<_>>()
        .try_collect()
        .await;
    HttpResponse::from(Result::from(result))
}
