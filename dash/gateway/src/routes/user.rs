use actix_web::{get, web::Data, HttpRequest, HttpResponse, Responder};
use ark_core::result::Result;
use kube::Client;
use vine_rbac::auth::UserSessionRef;

#[get("/user/")]
pub async fn get(request: HttpRequest, kube: Data<Client>) -> impl Responder {
    let kube = kube.as_ref().clone();
    let session = UserSessionRef::from_request(&kube, &request).await;
    HttpResponse::from(Result::from(session))
}
