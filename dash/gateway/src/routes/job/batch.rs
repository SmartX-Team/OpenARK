use std::collections::BTreeMap;

use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_provider_api::job::Payload;
use dash_provider_client::DashProviderClient;
use futures::future::try_join_all;
use kube::Client;
use serde_json::Value;
use vine_api::user_session::UserSessionMetadata;
use vine_rbac::auth::{AuthUserSession, AuthUserSessionMetadata};

#[post("/batch/job/")]
pub async fn post(
    request: HttpRequest,
    kube: Data<Client>,
    values: Json<Vec<Payload<BTreeMap<String, Value>>>>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let metadata = match UserSessionMetadata::from_request(&kube, &request).await {
        Ok(metadata) => metadata,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let result = try_join_all(values.0.into_iter().map(
        |Payload {
             task_name,
             namespace,
             value,
         }| {
            let kube = kube.clone();
            let metadata = metadata.clone();
            async move {
                let session = metadata.namespaced(namespace).await?;
                let client = DashProviderClient::new(kube, &session);
                client.create(&task_name, value).await
            }
        },
    ))
    .await;
    HttpResponse::from(Result::from(result))
}
