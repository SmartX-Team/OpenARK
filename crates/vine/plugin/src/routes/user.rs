use actix_web::{get, web::Data, HttpRequest, HttpResponse, Responder};
use ark_core::result::Result;
use kube::Client;
use tracing::{instrument, Level};
use vine_api::user_session::UserSession;
use vine_rbac::auth::AuthUserSession;

#[instrument(level = Level::INFO, skip(request, kube))]
#[get("/user")]
pub async fn get(request: HttpRequest, kube: Data<Client>) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = UserSession::from_request(&kube, &request).await;
    HttpResponse::from(Result::from(session))
}
