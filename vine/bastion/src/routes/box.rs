
pub mod login {
    use actix_web::{get, web::{Data, Path}, HttpRequest, HttpResponse, Responder};
    use ipis::log::error;
    use vine_api::{kube::Client, user_auth::UserLoginResponse};

    #[get("/box/{name}/login")]
    pub async fn get(request: HttpRequest, client: Data<Client>, name: Path<String>) -> impl Responder {
        match ::vine_rbac::login::execute(request, name.as_str(), client).await {
            Ok(response) if matches!(response, UserLoginResponse::Accept { .. }) => {
                HttpResponse::Ok().json(response)
            }
            Ok(response) => HttpResponse::Forbidden().json(response),
            Err(e) => {
                error!("failed to login: {e}");
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}