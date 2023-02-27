pub mod login {
    use actix_web::{
        get,
        web::{Data, Path, Redirect},
        HttpRequest, HttpResponse, Responder,
    };
    use ipis::{core::uuid::Uuid, log::error};
    use vine_api::{kube::Client, user_auth::UserLoginResponse};
    use vine_session::SessionManager;

    #[get("/box/{name}/login")]
    pub async fn get(
        request: HttpRequest,
        client: Data<Client>,
        session_manager: Data<SessionManager>,
        name: Path<Uuid>,
    ) -> impl Responder {
        match ::vine_rbac::login::execute(&request, &name.to_string(), client, session_manager)
            .await
        {
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
