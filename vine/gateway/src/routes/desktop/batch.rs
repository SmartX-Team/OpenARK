use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest, HttpResponse, Responder,
};
use ark_api::SessionRef;
use ark_core::result::Result;
use futures::future::try_join_all;
use kube::Client;
use vine_api::user_session::UserSessionMetadata;
use vine_rbac::auth::UserSessionMetadataRbac;
use vine_session::SessionExec;

#[post("/batch/user/desktop/exec/broadcast/")]
pub async fn post_exec_broadcast(
    request: HttpRequest,
    kube: Data<Client>,
    command: Json<Vec<String>>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    if let Err(error) = UserSessionMetadata::assert_admin(&kube, &request).await {
        return HttpResponse::from(Result::<()>::Err(error.to_string()));
    };

    let sessions = match SessionRef::list(kube.clone()).await {
        Ok(sessions) => sessions,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let command = command.0;
    let result = try_join_all(sessions.into_iter().map(|session| {
        let kube = kube.clone();
        let command = command.clone();
        async move { session.exec(kube, command).await.map(|_| ()) }
    }))
    .await
    .map(|_| ());
    HttpResponse::from(Result::from(result))
}
