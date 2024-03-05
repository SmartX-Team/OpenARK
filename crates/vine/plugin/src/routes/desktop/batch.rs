use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest, HttpResponse, Responder,
};
use ark_api::SessionRef;
use ark_core::result::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::Client;
use tracing::{instrument, Level};
use vine_api::user_session::{UserSessionCommandBatch, UserSessionMetadata};
use vine_rbac::auth::AuthUserSession;
use vine_session::SessionExec;

#[instrument(level = Level::INFO, skip(request, kube))]
#[post("/batch/user/desktop/exec/broadcast/")]
pub async fn post_exec_broadcast(
    request: HttpRequest,
    kube: Data<Client>,
    Json(UserSessionCommandBatch {
        command,
        user_names,
    }): Json<UserSessionCommandBatch>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    if let Err(error) = UserSessionMetadata::from_request(&kube, &request)
        .await
        .and_then(|metadata| metadata.assert_admin())
    {
        return HttpResponse::from(Result::<()>::Err(error.to_string()));
    };

    let sessions = match match user_names {
        Some(user_names) => SessionRef::load(kube.clone(), &user_names).await,
        None => SessionRef::list(kube.clone()).await,
    } {
        Ok(sessions) => sessions,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let result: ::core::result::Result<(), _> = sessions
        .into_iter()
        .map(|session| {
            let kube = kube.clone();
            let command = command.clone();
            async move { session.exec(kube, command).await.map(|_| ()) }
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect()
        .await;
    HttpResponse::from(Result::from(result))
}
