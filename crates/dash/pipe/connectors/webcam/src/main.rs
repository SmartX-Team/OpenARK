use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use dash_pipe_provider::{
    storage::StorageIO, FunctionContext, FunctionSignalExt, PipeArgs, PipeMessage, PipeMessages,
    PipePayload,
};
use derivative::Derivative;
use image::{codecs, RgbImage};
use opencv::{
    core::{Mat, MatTraitConst, MatTraitConstManual, Vec3b, Vector},
    imgcodecs,
    videoio::{self, VideoCapture, VideoCaptureTrait, VideoCaptureTraitConst},
};
use serde::{Deserialize, Serialize};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_DEVICE",
        value_name = "PATH",
        default_value_t = FunctionArgs::default_camera_device()
    )]
    #[serde(default = "FunctionArgs::default_camera_device")]
    camera_device: String,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_DECODER",
        value_name = "TYPE",
        value_enum,
        default_value_t = Default::default()
    )]
    #[serde(default)]
    camera_decoder: CameraDecoder,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_ENCODER",
        value_name = "TYPE",
        value_enum,
        default_value_t = Default::default()
    )]
    #[serde(default)]
    camera_encoder: CameraEncoder,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_FPS",
        value_name = "FPS",
        default_value_t = FunctionArgs::default_camera_fps()
    )]
    #[serde(default = "FunctionArgs::default_camera_fps")]
    camera_fps: f64,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_WIDTH",
        value_name = "SIZE",
        default_value_t = FunctionArgs::default_camera_width()
    )]
    #[serde(default = "FunctionArgs::default_camera_width")]
    camera_width: u32,

    #[arg(
        long,
        env = "PIPE_WEBCAM_CAMERA_HEIGHT",
        value_name = "SIZE",
        default_value_t = FunctionArgs::default_camera_height()
    )]
    #[serde(default = "FunctionArgs::default_camera_height")]
    camera_height: u32,
}

impl FunctionArgs {
    fn default_camera_device() -> String {
        "/dev/video0".into()
    }

    const fn default_camera_fps() -> f64 {
        60.0
    }

    const fn default_camera_width() -> u32 {
        1920
    }

    const fn default_camera_height() -> u32 {
        1080
    }
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
pub enum CameraDecoder {
    #[default]
    Jpeg,
}

impl CameraDecoder {
    const fn as_fourcc(&self) -> [char; 4] {
        match self {
            Self::Jpeg => ['M', 'J', 'P', 'G'],
        }
    }

    fn as_video_writer(&self) -> Result<f64> {
        let [c1, c2, c3, c4] = self.as_fourcc();
        videoio::VideoWriter::fourcc(c1, c2, c3, c4)
            .map(Into::into)
            .map_err(Into::into)
    }
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

impl PartialEq<CameraEncoder> for CameraDecoder {
    fn eq(&self, other: &CameraEncoder) -> bool {
        match self {
            Self::Jpeg => matches!(other, CameraEncoder::Jpeg),
        }
    }
}

impl PartialEq<CameraDecoder> for CameraEncoder {
    fn eq(&self, other: &CameraDecoder) -> bool {
        match self {
            Self::Bmp => false,
            Self::Jpeg => matches!(other, CameraDecoder::Jpeg),
            Self::Png => false,
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Function {
    camera_encoder: CameraEncoder,
    #[derivative(Debug = "ignore")]
    capture: VideoCapture,
    ctx: Option<FunctionContext>,
    #[derivative(Debug = "ignore")]
    frame: Mat,
    frame_counter: FrameCounter,
    frame_size: FrameSize,
    #[derivative(Debug = "ignore")]
    params: Vector<i32>,
}

pub type FunctionOutput = ::dash_openapi::image::Image;

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        ctx: Option<&mut FunctionContext>,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        let FunctionArgs {
            camera_device,
            camera_decoder,
            camera_encoder,
            camera_fps,
            camera_width,
            camera_height,
        } = args.clone();

        let mut capture = VideoCapture::from_file(&camera_device, videoio::CAP_ANY)
            .map_err(|error| anyhow!("failed to init video capture: {error}"))?;
        if !capture.is_opened().unwrap_or_default() {
            bail!("failed to open video capture");
        }

        capture.set(videoio::CAP_PROP_FOURCC, camera_decoder.as_video_writer()?)?;
        capture.set(videoio::CAP_PROP_FPS, camera_fps)?;
        capture.set(videoio::CAP_PROP_FRAME_WIDTH, camera_width.into())?;
        capture.set(videoio::CAP_PROP_FRAME_HEIGHT, camera_height.into())?;

        Ok(Self {
            camera_encoder,
            capture,
            ctx: ctx.map(|ctx| {
                ctx.disable_load();
                ctx.clone()
            }),
            frame: Default::default(),
            frame_counter: Default::default(),
            frame_size: Default::default(),
            params: Default::default(),
        })
    }
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Input = ();
    type Output = ::dash_openapi::image::Image;

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        let (frame, (width, height)) = match self.capture.read(&mut self.frame) {
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
                            Ok(true) => {
                                let frame = Vec::from(buffer).into();
                                let width = self.frame.cols().try_into().unwrap_or_default();
                                let height = self.frame.rows().try_into().unwrap_or_default();
                                (frame, (width, height))
                            }
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
                        .map(|()| (buffer.into(), (width, height)))
                        .map_err(|error| anyhow!("failed to encode image frame: {error}"))?
                    }
                }
            }
            Ok(false) => {
                let error = || anyhow!("video capture is disconnected!");
                return self
                    .ctx
                    .as_ref()
                    .map(|ctx| ctx.terminate_err(error()))
                    .unwrap_or_else(|| Err(error()));
            }
            Err(error) => bail!("failed to capture a frame: {error}"),
        };

        let frame_idx = self.frame_counter.next();
        let payloads = vec![PipePayload::new(
            format!(
                "images/{frame_idx:06}{ext}",
                ext = self.camera_encoder.as_extension(),
            ),
            Some(frame),
        )];
        let value = FunctionOutput {
            width,
            height,
            index: frame_idx,
        };

        Ok(PipeMessages::Single(PipeMessage::with_payloads(
            payloads, value,
        )))
    }
}

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
struct FrameCounter(usize);

impl FrameCounter {
    fn next(&mut self) -> usize {
        let index = self.0;
        self.0 += 1;
        index
    }
}
