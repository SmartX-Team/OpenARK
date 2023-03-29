use std::collections::BTreeMap;

use actix_form_data::Value;
use bytes::Bytes;
use image::{imageops::FilterType, GenericImageView, Pixel};
use ipis::core::{
    anyhow::{bail, Error, Result},
    ndarray::Array,
};
use ort::{
    session::{Input, Output},
    tensor::{InputTensor, TensorElementDataType},
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

pub trait ToTensor {
    fn to_tensor(&self, kind: &TensorKind) -> Result<InputTensor>;

    fn to_tensor_map(&self, kinds: &TensorKindMap) -> Result<TensorMap>;
}

impl ToTensor for Value<Bytes> {
    fn to_tensor(&self, kind: &TensorKind) -> Result<InputTensor> {
        match self {
            Self::Bytes(data) => kind.convert_bytes(data),
            Self::Text(data) => match kind {
                TensorKind::Text(kind) => kind.convert_str(data),
                kind => {
                    let type_ = kind.type_();
                    bail!("expected {type_}, but given Text")
                }
            },
            Self::File(data) => kind.convert_bytes(&data.result),
            _ => bail!("unsupported value"),
        }
    }

    fn to_tensor_map(&self, kinds: &TensorKindMap) -> Result<TensorMap> {
        todo!()
    }
}

pub type TensorMap = BTreeMap<String, InputTensor>;

pub type TensorKindMap = BTreeMap<String, TensorKind>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "spec")]
pub enum TensorKind {
    Text(#[serde(default)] TextKind),
    Image(#[serde(default)] ImageKind),
}

impl TryFrom<&Input> for TensorKind {
    type Error = Error;

    fn try_from(value: &Input) -> Result<Self, Self::Error> {
        Self::try_from_ort(&value.name, &value.dimensions, value.input_type)
    }
}

impl TryFrom<&Output> for TensorKind {
    type Error = Error;

    fn try_from(value: &Output) -> Result<Self, Self::Error> {
        Self::try_from_ort(&value.name, &value.dimensions, value.output_type)
    }
}

impl TensorKind {
    fn try_from_ort(
        name: &str,
        dimensions: &[Option<u32>],
        type_: TensorElementDataType,
    ) -> Result<Self> {
        let fail = || bail!("unsupported kind: {name:?}");

        match dimensions.len() {
            2 => match type_ {
                TensorElementDataType::Int64
                | TensorElementDataType::Float32
                | TensorElementDataType::Float64 => Ok(Self::Text(TextKind {
                    tensor_type: type_.into(),
                    max_len: dimensions[1],
                })),
                _ => fail(),
            },
            4 => match type_ {
                TensorElementDataType::Uint8 | TensorElementDataType::Float32 => {
                    // NCHW format
                    Ok(Self::Image(ImageKind {
                        tensor_type: type_.into(),
                        channels: dimensions[1].try_into()?, // C
                        width: dimensions[3],                // W
                        height: dimensions[2],               // H
                    }))
                }
                _ => fail(),
            },
            _ => fail(),
        }
    }

    fn convert_bytes(&self, bytes: &Bytes) -> Result<InputTensor> {
        match self {
            Self::Text(kind) => kind.convert_bytes(bytes),
            Self::Image(kind) => kind.convert_bytes(bytes),
        }
    }

    fn type_(&self) -> TensorKindType {
        match self {
            Self::Text(_) => TensorKindType::Text,
            Self::Image(_) => TensorKindType::Image,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextKind {
    tensor_type: TensorType,
    max_len: Option<u32>,
}

impl TextKind {
    fn convert_bytes(&self, bytes: &Bytes) -> Result<InputTensor> {
        todo!()
    }

    fn convert_str(&self, s: &str) -> Result<InputTensor> {
        todo!()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageKind {
    tensor_type: TensorType,
    channels: ImageChannel,
    width: Option<u32>,
    height: Option<u32>,
}

impl ImageKind {
    fn convert_bytes(&self, bytes: &Bytes) -> Result<InputTensor> {
        fn convert_image<I>(
            image: I,
            tensor_type: TensorType,
            shape: (usize, usize, usize, usize),
        ) -> InputTensor
        where
            I: GenericImageView,
            <I as GenericImageView>::Pixel: Pixel<Subpixel = u8>,
        {
            let get_pixel = |(_, c, y, x)| {
                let pixel = image.get_pixel(x as u32, y as u32);
                let channels = pixel.channels();
                channels[c]
            };

            match tensor_type {
                TensorType::Uint8 => {
                    InputTensor::Uint8Tensor(Array::from_shape_fn(shape, get_pixel).into_dyn())
                }
                TensorType::Float32 => InputTensor::FloatTensor(
                    Array::from_shape_fn(shape, |idx| (get_pixel(idx) as f32) / 255.0).into_dyn(),
                ),
                _ => unreachable!("unsupported tensor type: {tensor_type:?}"),
            }
        }

        const RESIZE_FILTER: FilterType = FilterType::Nearest;

        let image = image::load_from_memory(bytes)?;

        let image = match (self.width, self.height) {
            (Some(width), Some(height)) => image.resize_exact(width, height, RESIZE_FILTER),
            (Some(_), None) | (None, Some(_)) => bail!("scaling an image is not supported yet."),
            (None, None) => image,
        };

        let tensor_type = self.tensor_type;
        let width = image.width() as usize;
        let height = image.height() as usize;

        let get_image_shape = |c| (1, c, width, height);
        Ok(match self.channels {
            ImageChannel::L8 => convert_image(image.to_luma8(), tensor_type, get_image_shape(1)),
            ImageChannel::La8 => {
                convert_image(image.to_luma_alpha8(), tensor_type, get_image_shape(2))
            }
            ImageChannel::Rgb8 => convert_image(image.to_rgb8(), tensor_type, get_image_shape(3)),
            ImageChannel::Rgba8 => convert_image(image.to_rgba8(), tensor_type, get_image_shape(4)),
        })
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum ImageChannel {
    L8,
    La8,
    Rgb8,
    Rgba8,
}

impl TryFrom<Option<u32>> for ImageChannel {
    type Error = Error;

    fn try_from(value: Option<u32>) -> Result<Self, Self::Error> {
        Ok(match value {
            None => bail!("dynamic image channels are not supported"),
            Some(0) => bail!("zero-sized image channels are not supported"),
            Some(1) => Self::L8,
            Some(2) => Self::La8,
            Some(3) => Self::Rgb8,
            Some(4) => Self::Rgba8,
            Some(c) => bail!("too high image channels: {c:?}"),
        })
    }
}

impl Default for ImageChannel {
    fn default() -> Self {
        Self::Rgb8
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum TensorKindType {
    Text,
    Image,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum TensorType {
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Bfloat16,
    Float16,
    Float32,
    Float64,
    String,
}

impl From<TensorElementDataType> for TensorType {
    fn from(value: TensorElementDataType) -> Self {
        match value {
            TensorElementDataType::Bool => Self::Bool,
            TensorElementDataType::Int8 => Self::Int8,
            TensorElementDataType::Int16 => Self::Int16,
            TensorElementDataType::Int32 => Self::Int32,
            TensorElementDataType::Int64 => Self::Int64,
            TensorElementDataType::Uint8 => Self::Uint8,
            TensorElementDataType::Uint16 => Self::Uint16,
            TensorElementDataType::Uint32 => Self::Uint32,
            TensorElementDataType::Uint64 => Self::Uint64,
            TensorElementDataType::Bfloat16 => Self::Bfloat16,
            TensorElementDataType::Float16 => Self::Float16,
            TensorElementDataType::Float32 => Self::Float32,
            TensorElementDataType::Float64 => Self::Float64,
            TensorElementDataType::String => Self::String,
        }
    }
}

impl From<TensorType> for TensorElementDataType {
    fn from(value: TensorType) -> Self {
        match value {
            TensorType::Bool => Self::Bool,
            TensorType::Int8 => Self::Int8,
            TensorType::Int16 => Self::Int16,
            TensorType::Int32 => Self::Int32,
            TensorType::Int64 => Self::Int64,
            TensorType::Uint8 => Self::Uint8,
            TensorType::Uint16 => Self::Uint16,
            TensorType::Uint32 => Self::Uint32,
            TensorType::Uint64 => Self::Uint64,
            TensorType::Bfloat16 => Self::Bfloat16,
            TensorType::Float16 => Self::Float16,
            TensorType::Float32 => Self::Float32,
            TensorType::Float64 => Self::Float64,
            TensorType::String => Self::String,
        }
    }
}

impl Default for TensorType {
    fn default() -> Self {
        Self::Float32
    }
}
