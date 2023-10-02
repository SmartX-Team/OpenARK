use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use bytes::Bytes;
use clap::{Parser, ValueEnum};
use dash_pipe_provider::{PipeArgs, PipeMessage, PipeMessages, PipePayload};
use image::{codecs, RgbImage};
use log::warn;
use opencv::{
    core::{Mat, MatTraitConst, MatTraitConstManual, Vec3b, Vector},
    imgcodecs,
    videoio::{self, VideoCapture, VideoCaptureTrait},
};
use serde::{Deserialize, Serialize};
use tokio::{
    spawn,
    sync::mpsc::{self, Receiver},
    task::{yield_now, JoinHandle},
    time::sleep,
};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_WEBCAM_CAMERA_DEVICE", value_name = "PATH")]
    camera_device: String,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_ENCODER",
        value_name = "TYPE",
        value_enum,
        default_value_t = CameraEncoder::default()
    )]
    #[serde(default)]
    camera_encoder: CameraEncoder,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    ValueEnum,
)]
#[serde(rename_all = "camelCase")]
pub enum CameraEncoder {
    Bmp,
    #[default]
    Jpeg,
    Png,
}

impl CameraEncoder {
    const fn as_extension(&self) -> &'static str {
        match self {
            CameraEncoder::Bmp => ".bmp",
            CameraEncoder::Jpeg => ".jpeg",
            CameraEncoder::Png => ".png",
        }
    }
}

pub struct Function {
    encoder: CameraEncoder,
    frame: FrameCounter,
    job: Option<JoinHandle<Result<()>>>,
    rx: Receiver<Bytes>,
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = ();
    type Output = usize;

    async fn try_new(args: &<Self as ::dash_pipe_provider::Function>::Args) -> Result<Self> {
        let FunctionArgs {
            camera_device,
            camera_encoder,
        } = args.clone();
        let (tx, rx) = mpsc::channel(3);

        Ok(Self {
            encoder: camera_encoder,
            frame: Default::default(),
            job: Some(spawn(async move {
                let mut capture = VideoCapture::from_file(&camera_device, videoio::CAP_ANY)?;
                let params = Vector::default();

                let mut frame = Mat::default();
                let mut frame_size = FrameSize::default();
                loop {
                    match capture.read(&mut frame) {
                        Ok(true) => {
                            let buffer = match camera_encoder {
                                CameraEncoder::Bmp | CameraEncoder::Jpeg => {
                                    // convert image
                                    let mut buffer = Default::default();
                                    match imgcodecs::imencode(
                                        camera_encoder.as_extension(),
                                        &frame,
                                        &mut buffer,
                                        &params,
                                    ) {
                                        Ok(true) => buffer.into(),
                                        Ok(false) => bail!("failed to encode image frame"),
                                        Err(error) => {
                                            bail!("failed to encode image frame: {error}")
                                        }
                                    }
                                }
                                CameraEncoder::Png => {
                                    // load image
                                    let buffer = Mat::data_typed::<Vec3b>(&frame)
                                        .map_err(|error| {
                                            anyhow!("failed to catch frame data type: {error}")
                                        })?
                                        .iter()
                                        .flat_map(|pixel| {
                                            let [p1, p2, p3] = pixel.0;
                                            [p3, p2, p1]
                                        })
                                        .collect();

                                    // parse image
                                    let (width, height) = frame_size.get_or_insert(&frame);
                                    let image = RgbImage::from_raw(width, height, buffer)
                                        .ok_or_else(|| {
                                            anyhow!("failed to get sufficient frame data")
                                        })?;

                                    // encode image
                                    let mut buffer = vec![];
                                    match camera_encoder {
                                        CameraEncoder::Bmp | CameraEncoder::Jpeg => unreachable!(
                                            "unsupported image codec for native image crate"
                                        ),
                                        CameraEncoder::Png => image.write_with_encoder(
                                            codecs::png::PngEncoder::new(&mut buffer),
                                        ),
                                    }
                                    .map(|()| buffer)
                                    .map_err(|error| {
                                        anyhow!("failed to encode image frame: {error}")
                                    })?
                                }
                            };

                            match tx.send(buffer.into()).await {
                                Ok(()) => yield_now().await,
                                Err(error) => bail!("failed to get frame: {error}"),
                            }
                        }
                        Ok(false) => bail!("video capture is disconnected!"),
                        Err(error) => bail!("failed to capture a frame: {error}"),
                    }
                }
            })),
            rx,
        })
    }

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        match &mut self.job {
            Some(job) => {
                if job.is_finished() {
                    match self.job.take().unwrap().await {
                        Ok(Ok(())) => {
                            unreachable!("the producer job should not be gracefully terminated")
                        }
                        Ok(Err(error)) => {
                            warn!("failed on producer job: {error}");
                            sleep(Duration::from_millis(u64::MAX)).await;
                            Ok(PipeMessages::None)
                        }
                        Err(error) => panic!("failed to terminate producer job: {error}"),
                    }
                } else {
                    match self.rx.recv().await {
                        Some(frame) => {
                            let frame_idx = self.frame.next();
                            Ok(PipeMessages::Single(PipeMessage {
                                payloads: vec![PipePayload::new(
                                    format!(
                                        "image/{frame_idx:06}{ext}",
                                        ext = self.encoder.as_extension(),
                                    ),
                                    frame,
                                )],
                                value: frame_idx,
                            }))
                        }
                        None => Ok(PipeMessages::None),
                    }
                }
            }
            None => unreachable!("the process should be exited"),
        }
    }
}

#[derive(Default)]
struct FrameSize(Option<(u32, u32)>);

impl FrameSize {
    fn get_or_insert(&mut self, frame: &Mat) -> (u32, u32) {
        match self.0 {
            Some(size) => size,
            None => {
                let width = frame.cols() as u32;
                let height = frame.rows() as u32;
                let size = (width, height);

                self.0.replace(size);
                size
            }
        }
    }
}

#[derive(Default)]
struct FrameCounter(usize);

impl FrameCounter {
    fn next(&mut self) -> usize {
        let index = self.0;
        self.0 += 1;
        index
    }
}
