use actix_web::{get, HttpResponse, Responder};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/print/reserved")]
pub async fn get() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/reserved.html"))
}
