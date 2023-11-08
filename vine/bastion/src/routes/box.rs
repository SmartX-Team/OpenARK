pub mod login {
    use actix_web::{
        get,
        web::{Data, Path, Redirect},
        HttpRequest, HttpResponse, Responder,
    };
    use kube::Client;
    use tracing::{error, instrument, Level};
    use uuid::Uuid;
    use vine_api::user_auth::UserSessionResponse;

    #[instrument(level = Level::INFO, skip(request, client))]
    #[get("/box/{box_name}/login")]
    pub async fn get(
        request: HttpRequest,
        client: Data<Client>,
        box_name: Path<Uuid>,
    ) -> impl Responder {
        match {
            match ::vine_rbac::auth::get_user_name(&request) {
                Ok(user_name) => {
                    ::vine_rbac::login::execute(&client, &box_name.to_string(), &user_name).await
                }
                Err(response) => Ok(response.into()),
            }
        } {
            Ok(UserSessionResponse::Accept { .. }) => Redirect::to("../../")
                .temporary()
                .respond_to(&request)
                .map_into_boxed_body(),
            Ok(response) => HttpResponse::Forbidden().json(response),
            Err(e) => {
                error!("failed to login: {e}");
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}
