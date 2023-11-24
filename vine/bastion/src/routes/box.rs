pub mod login {
    use actix_web::{
        get,
        web::{Data, Path, Redirect},
        HttpRequest, HttpResponse, Responder,
    };
    use kube::Client;
    use serde::Serialize;
    use tera::{Context, Tera};
    use tracing::{error, instrument, warn, Level};
    use uuid::Uuid;
    use vine_api::user_auth::UserSessionResponse;

    pub const TEMPLATE_NAME: &str = "box_error.html";
    pub const TEMPLATE_CONTENT: &str = include_str!("../../templates/box_error.html.j2");

    #[instrument(level = Level::INFO, skip(request, client, tera))]
    #[get("/box/{box_name}/login")]
    pub async fn get(
        request: HttpRequest,
        client: Data<Client>,
        tera: Data<Tera>,
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
            Ok(UserSessionResponse::Error(error)) => {
                warn!("denied to login: {error}");
                create_error_html(tera, error)
            }
            Err(error) => {
                error!("failed to login: {error}");
                create_error_html(
                    tera,
                    "Internal server error. Please contact the administrator.",
                )
            }
        }
    }

    fn create_error_html(tera: Data<Tera>, error: impl ToString) -> HttpResponse {
        #[derive(Serialize)]
        struct Value {
            error: String,
        }

        let value = Value {
            error: error.to_string(),
        };

        match Context::from_serialize(value)
            .and_then(|context| tera.render(TEMPLATE_NAME, &context))
        {
            Ok(body) => HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(body),
            Err(error) => {
                error!("failed to render box error: {error}");
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}
