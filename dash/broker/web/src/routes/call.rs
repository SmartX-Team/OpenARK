use actix_web::{
    get, post,
    web::{Data, Json, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_pipe_provider::{MaybePipeMessage, PipeClient};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(ctx))]
#[get("/call/{topic:.*}")]
pub async fn get(ctx: Data<PipeClient>, topic: Path<String>) -> impl Responder {
    match topic.replace('/', ".").parse() {
        Ok(topic) => HttpResponse::from(Result::from(ctx.read(topic).await)),
        Err(error) => HttpResponse::from(Result::<()>::Err(error.to_string())),
    }
}

#[instrument(level = Level::INFO, skip(ctx, message))]
#[post("/call/{topic:.*}")]
pub async fn post(
    ctx: Data<PipeClient>,
    topic: Path<String>,
    message: Json<MaybePipeMessage>,
) -> impl Responder {
    match topic.replace('/', ".").parse() {
        Ok(topic) => HttpResponse::from(Result::from(
            ctx.call(topic, message.into_inner().into()).await,
        )),
        Err(error) => HttpResponse::from(Result::<()>::Err(error.to_string())),
    }
}
