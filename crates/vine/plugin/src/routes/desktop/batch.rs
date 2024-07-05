use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use kube::Client;
use tracing::{instrument, Level};
use vine_api::user_session::{UserSessionCommandBatch, UserSessionMetadata};
use vine_rbac::auth::AuthUserSession;
use vine_session::batch::{BatchCommandArgs, BatchCommandUsers};

#[instrument(level = Level::INFO, skip(request, kube))]
#[post("/batch/user/desktop/exec/broadcast")]
pub async fn post_exec_broadcast(
    request: HttpRequest,
    kube: Data<Client>,
    Json(UserSessionCommandBatch {
        command,
        terminal,
        user_names,
        wait,
    }): Json<UserSessionCommandBatch>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    if let Err(error) = UserSessionMetadata::from_request(&kube, &request)
        .await
        .and_then(|metadata| metadata.assert_admin())
    {
        return HttpResponse::from(Result::<()>::Err(error.to_string()));
    };

    let args = BatchCommandArgs {
        command,
        terminal,
        users: match user_names {
            Some(user_names) => BatchCommandUsers::List(user_names),
            None => BatchCommandUsers::All,
        },
        wait,
    };

    let result = args.exec(&kube).await;
    HttpResponse::from(Result::from(result))
}
