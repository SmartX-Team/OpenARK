use actix_web::{
    get, post,
    web::{Data, Json, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use dash_pipe_provider::{DynValue, MaybePipeMessage, PipeClient};
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(client))]
#[get("/r/{topic:.*}")]
pub async fn get(client: Data<PipeClient>, topic: Path<String>) -> impl Responder {
    match topic.replace('/', ".").parse() {
        Ok(topic) => HttpResponse::from(Result::from(client.read(topic).await)),
        Err(error) => HttpResponse::from(Result::<()>::Err(error.to_string())),
    }
}

#[instrument(level = Level::INFO, skip(client, message))]
#[post("/r/{topic:.*}")]
pub async fn post(
    client: Data<PipeClient>,
    topic: Path<String>,
    message: Json<MaybePipeMessage>,
) -> impl Responder {
    match topic.replace('/', ".").parse() {
        Ok(topic) => HttpResponse::from(Result::from(
            client
                .call::<DynValue>(topic, message.into_inner().into())
                .await,
        )),
        Err(error) => HttpResponse::from(Result::<()>::Err(error.to_string())),
    }
}
