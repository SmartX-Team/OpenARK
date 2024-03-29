use actix_web::{get, HttpResponse, Responder};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/print/install_os")]
pub async fn get() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/install_os.html"))
}
