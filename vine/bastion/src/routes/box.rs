pub mod login {
    use actix_web::{
        get,
        web::{Data, Path, Redirect},
        HttpRequest, HttpResponse, Responder,
    };
    use kube::Client;
    use log::error;
    use uuid::Uuid;
    use vine_api::user_auth::UserLoginResponse;

    #[get("/box/{name}/login")]
    pub async fn get(
        request: HttpRequest,
        client: Data<Client>,
        name: Path<Uuid>,
    ) -> impl Responder {
        match ::vine_rbac::login::execute(&request, &name.to_string(), client).await {
            Ok(response) if matches!(response, UserLoginResponse::Accept { .. }) => {
                Redirect::to("../../")
                    .temporary()
                    .respond_to(&request)
                    .map_into_boxed_body()
            }
            Ok(response) => HttpResponse::Forbidden().json(response),
            Err(e) => {
                error!("failed to login: {e}");
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}
