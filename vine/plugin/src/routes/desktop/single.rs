use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use kube::Client;
use vine_api::user_session::UserSessionRef;
use vine_rbac::auth::{AuthUserSession, AuthUserSessionRef};
use vine_session::SessionExec;

#[post("/user/desktop/exec/")]
pub async fn post_exec(
    request: HttpRequest,
    kube: Data<Client>,
    command: Json<Vec<String>>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = match UserSessionRef::from_request(&kube, &request)
        .await
        .and_then(|session| session.try_into_ark_session())
    {
        Ok(session) => session,
        Err(error) => return HttpResponse::from(Result::<()>::Err(error.to_string())),
    };

    let command = command.0;
    let result = session.exec(kube, command).await.map(|_| ());
    HttpResponse::from(Result::from(result))
}
