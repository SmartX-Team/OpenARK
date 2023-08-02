use actix_web::{
    post,
    web::{Data, Path},
    HttpRequest, HttpResponse, Responder,
};
use ark_core::result::Result;
use kiss_api::r#box::BoxPowerAction;
use kube::Client;
use vine_api::user_session::UserSessionMetadata;
use vine_rbac::auth::AuthUserSession;

#[post("/user/{action}")]
pub async fn post(
    request: HttpRequest,
    kube: Data<Client>,
    action: Path<BoxPowerAction>,
) -> impl Responder {
    let kube = kube.as_ref().clone();
    if let Err(error) = UserSessionMetadata::from_request(&kube, &request)
        .await
        .and_then(|metadata| metadata.assert_admin())
    {
        return HttpResponse::from(Result::<()>::Err(error.to_string()));
    };

    #[allow(unreachable_code)]
    match action.into_inner() {}
}
