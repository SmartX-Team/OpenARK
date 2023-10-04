use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use dash_pipe_provider::{PipeArgs, PipeMessage, PipeMessages, PipePayload};
use image::{codecs, RgbImage};
use opencv::{
    core::{Mat, MatTraitConst, MatTraitConstManual, Vec3b, Vector},
    imgcodecs,
    videoio::{self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst},
};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::error;

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_WEBCAM_CAMERA_DEVICE", value_name = "PATH")]
    camera_device: String,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_ENCODER",
        value_name = "TYPE",
        value_enum,
        default_value_t = Default::default()
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
    camera_encoder: CameraEncoder,
    capture: VideoCapture,
    frame: Mat,
    frame_counter: FrameCounter,
    frame_size: FrameSize,
    params: Vector<i32>,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = ();
    type Output = usize;

    async fn try_new(args: &<Self as ::dash_pipe_provider::Function>::Args) -> Result<Self> {
        let FunctionArgs {
            camera_device,
            camera_encoder,
        } = args.clone();

        let capture = VideoCapture::from_file(&camera_device, videoio::CAP_ANY)
            .map_err(|error| anyhow!("failed to init video capture: {error}"))?;
        if !capture.is_opened().unwrap_or_default() {
            bail!("failed to open video capture");
        }

        Ok(Self {
            camera_encoder,
            capture,
            frame: Default::default(),
            frame_counter: Default::default(),
            frame_size: Default::default(),
            params: Default::default(),
        })
    }

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        let frame = match self.capture.read(&mut self.frame) {
            Ok(true) => {
                match self.camera_encoder {
                    CameraEncoder::Bmp | CameraEncoder::Jpeg => {
                        // convert image
                        let mut buffer = Default::default();
                        match imgcodecs::imencode(
                            self.camera_encoder.as_extension(),
                            &self.frame,
                            &mut buffer,
                            &self.params,
                        ) {
                            Ok(true) => Vec::from(buffer).into(),
                            Ok(false) => bail!("failed to encode image frame"),
                            Err(error) => {
                                bail!("failed to encode image frame: {error}")
                            }
                        }
                    }
                    CameraEncoder::Png => {
                        // load image
                        let buffer = Mat::data_typed::<Vec3b>(&self.frame)
                            .map_err(|error| anyhow!("failed to catch frame data type: {error}"))?
                            .iter()
                            .flat_map(|pixel| {
                                let [p1, p2, p3] = pixel.0;
                                [p3, p2, p1]
                            })
                            .collect();

                        // parse image
                        let (width, height) = self.frame_size.get_or_insert(&self.frame);
                        let image = RgbImage::from_raw(width, height, buffer)
                            .ok_or_else(|| anyhow!("failed to get sufficient frame data"))?;

                        // encode image
                        let mut buffer = vec![];
                        match self.camera_encoder {
                            CameraEncoder::Bmp | CameraEncoder::Jpeg => {
                                unreachable!("unsupported image codec for native image crate")
                            }
                            CameraEncoder::Png => {
                                image.write_with_encoder(codecs::png::PngEncoder::new(&mut buffer))
                            }
                        }
                        .map(|()| buffer.into())
                        .map_err(|error| anyhow!("failed to encode image frame: {error}"))?
                    }
                }
            }
            Ok(false) => {
                error!("video capture is disconnected!");
                sleep(Duration::from_millis(u64::MAX)).await;
                return Ok(PipeMessages::None);
            }
            Err(error) => bail!("failed to capture a frame: {error}"),
        };

        let frame_idx = self.frame_counter.next();
        Ok(PipeMessages::Single(PipeMessage {
            payloads: vec![PipePayload::new(
                format!(
                    "image/{frame_idx:06}{ext}",
                    ext = self.camera_encoder.as_extension(),
                ),
                frame,
            )],
            value: frame_idx,
        }))
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
