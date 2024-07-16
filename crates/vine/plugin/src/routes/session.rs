use actix_web::{get, web::Data, HttpRequest, HttpResponse, Responder};
use ark_api::SessionRef;
use ark_core::result::Result;
use kube::Client;
use tracing::{instrument, warn, Level};
use vine_api::user_session::UserSessionMetadata;
use vine_rbac::auth::AuthUserSession;
use vine_session::exec::SessionExec;

#[instrument(level = Level::INFO, skip(request, kube))]
#[get("/batch/user/session")]
pub async fn list(request: HttpRequest, kube: Data<Client>) -> impl Responder {
    let kube = kube.as_ref().clone();
    if let Err(error) = UserSessionMetadata::from_request(&kube, &request)
        .await
        .and_then(|metadata| metadata.assert_admin())
    {
        warn!("{error}");
        return HttpResponse::from(Result::<()>::Err(error.to_string()));
    };

    HttpResponse::from(Result::from(SessionRef::list(kube.clone()).await.map(
        |sessions| {
            sessions
                .into_iter()
                .map(SessionRef::into_owned)
                .collect::<Vec<_>>()
        },
    )))
}
