use actix_web::{get, HttpResponse, Responder};

#[get("/")]
pub async fn get() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../../static/welcome.html"))
}
