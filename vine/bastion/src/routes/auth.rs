use actix_web::{get, web::Data, HttpRequest, HttpResponse, Responder};
use ipis::log::error;
use vine_api::{kube::Client, user_auth::UserAuthResponse};

#[get("/auth")]
pub async fn get(request: HttpRequest, client: Data<Client>) -> impl Responder {
    match ::vine_rbac::auth::execute(&request, client).await {
        Ok(response) if matches!(response, UserAuthResponse::Accept { .. }) => {
            HttpResponse::Ok().json(response)
        }
        Ok(response) => HttpResponse::Forbidden().json(response),
        Err(e) => {
            error!("failed to auth: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}
