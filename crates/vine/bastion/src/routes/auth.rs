use actix_web::{get, web::Data, HttpRequest, HttpResponse, Responder};
use kube::Client;
use tracing::{error, instrument, Level};
use vine_api::user_auth::UserAuthResponse;

#[instrument(level = Level::INFO, skip(request, client))]
#[get("/auth")]
pub async fn get(request: HttpRequest, client: Data<Client>) -> impl Responder {
    match match ::vine_rbac::auth::get_user_name(&request) {
        Ok(user_name) => ::vine_rbac::auth::execute(&client, &user_name).await,
        Err(response) => Ok(response.into()),
    } {
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
