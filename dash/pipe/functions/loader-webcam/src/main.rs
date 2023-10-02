use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{PipeArgs, PipeMessage, PipeMessages, PipePayload};
use nokhwa::{
    pixel_format,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    Buffer, CallbackCamera,
};
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::Handle,
    sync::mpsc::{self, Receiver},
};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_WEBCAM_CAMERA_DEVICE", value_name = "PATH")]
    camera_device: String,
}

pub struct Function {
    _camera: CallbackCamera,
    rx: Receiver<Buffer>,
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = ();
    type Output = ();

    async fn try_new(args: &<Self as ::dash_pipe_provider::Function>::Args) -> Result<Self> {
        let (tx, rx) = mpsc::channel(3);
        let handle = Handle::current();

        Ok(Self {
            _camera: CallbackCamera::new(
                CameraIndex::String(args.camera_device.clone()),
                RequestedFormat::new::<pixel_format::RgbFormat>(
                    RequestedFormatType::HighestFrameRate(30),
                ),
                move |frame| {
                    handle.block_on(tx.send(frame)).ok();
                },
            )?,
            rx,
        })
    }

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        match self.rx.recv().await {
            Some(buffer) => Ok(PipeMessages::Single(PipeMessage {
                payloads: vec![PipePayload::new("image".into(), buffer.buffer_bytes())],
                value: (),
            })),
            None => Ok(PipeMessages::None),
        }
    }
}
