use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use clap::Parser;
use dash_pipe_provider::{PipeArgs, PipeMessage, PipeMessages, PipePayload};
use opencv::{
    core::{Mat, MatTraitConstManual, Vec3b},
    videoio::{self, VideoCapture, VideoCaptureTrait},
};
use serde::{Deserialize, Serialize};
use tokio::{
    spawn,
    sync::mpsc::{self, Receiver},
    task::{yield_now, JoinHandle},
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
    _job: JoinHandle<Result<()>>,
    rx: Receiver<Bytes>,
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = ();
    type Output = ();

    async fn try_new(args: &<Self as ::dash_pipe_provider::Function>::Args) -> Result<Self> {
        let FunctionArgs { camera_device } = args.clone();
        let (tx, rx) = mpsc::channel(3);

        Ok(Self {
            _job: spawn(async move {
                let mut capture = VideoCapture::from_file(&camera_device, videoio::CAP_ANY)?;

                let mut frame = Mat::default();
                loop {
                    // yield per every loop
                    yield_now().await;

                    match capture.read(&mut frame) {
                        Ok(true) => {
                            let pixels = Mat::data_typed::<Vec3b>(&frame).map_err(|error| {
                                anyhow!("failed to convert video capture frame: {error}")
                            })?;

                            let mut buffer = BytesMut::default();
                            for pixel in pixels {
                                buffer.extend(pixel.iter().rev());
                            }

                            if let Err(error) = tx.send(buffer.into()).await {
                                bail!("failed to get frame: {error}");
                            }
                        }
                        Ok(false) => bail!("video capture is disconnected!"),
                        Err(error) => bail!("failed to capture a frame: {error}"),
                    }
                }
            }),
            rx,
        })
    }

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        match self.rx.recv().await {
            Some(frame) => Ok(PipeMessages::Single(PipeMessage {
                payloads: vec![PipePayload::new("image".into(), frame)],
                value: (),
            })),
            None => Ok(PipeMessages::None),
        }
    }
}
