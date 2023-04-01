use std::collections::BTreeMap;

use image::{imageops::FilterType, GenericImageView, Pixel};
use ipis::{
    async_trait::async_trait,
    core::{
        anyhow::{anyhow, bail, Error, Result},
        ndarray::{self, Array, Array1, ArrayView, Axis, IxDyn},
    },
    futures::{future::try_join_all, TryFutureExt},
    itertools::Itertools,
    tokio::{self, io::AsyncReadExt},
};
use ort::{
    session::{Input, Output},
    tensor::{InputTensor, TensorElementDataType},
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[async_trait]
pub trait ToTensor
where
    Self: Send,
{
    type Field: ?Sized;
    type Output;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output>;
}

#[async_trait]
impl ToTensor for ::actix_multipart::form::bytes::Bytes {
    type Field = TensorField;
    type Output = InputTensor;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        field.convert_bytes(&self.data)
    }
}

#[async_trait]
impl ToTensor for ::actix_multipart::form::tempfile::TempFile {
    type Field = TensorField;
    type Output = InputTensor;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        let mut file = tokio::fs::File::from_std(self.file.into_file());
        let mut buf = Default::default();
        file.read_to_end(&mut buf).await?;

        field.convert_bytes(&buf)
    }
}

#[async_trait]
impl ToTensor for ::actix_multipart::form::text::Text<String> {
    type Field = TensorField;
    type Output = InputTensor;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        match &field.kind {
            TensorKind::Text(kind) => kind.convert_string(self.0),
            kind => {
                let type_ = kind.type_();
                bail!("expected {type_}, but given Text")
            }
        }
    }
}

#[async_trait]
impl<T> ToTensor for Vec<T>
where
    T: ToTensor<Field = TensorField, Output = InputTensor>,
{
    type Field = TensorField;
    type Output = InputTensor;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        let array = try_join_all(self.into_iter().map(|item| item.into_tensor(field))).await?;

        if array.is_empty() {
            bail!("failed to parse zero-sized tensor");
        }

        match &array[0] {
            InputTensor::Int8Tensor(_) => i8::unwrap_tensor_array(&array),
            InputTensor::Int16Tensor(_) => i16::unwrap_tensor_array(&array),
            InputTensor::Int32Tensor(_) => i32::unwrap_tensor_array(&array),
            InputTensor::Int64Tensor(_) => i64::unwrap_tensor_array(&array),
            InputTensor::Uint8Tensor(_) => u8::unwrap_tensor_array(&array),
            InputTensor::Uint16Tensor(_) => u16::unwrap_tensor_array(&array),
            InputTensor::Uint32Tensor(_) => u32::unwrap_tensor_array(&array),
            InputTensor::Uint64Tensor(_) => u64::unwrap_tensor_array(&array),
            InputTensor::Bfloat16Tensor(_) => {
                bail!("concatenating Bfloat16Tensors are not supported yet")
            }
            InputTensor::Float16Tensor(_) => {
                bail!("concatenating Float16Tensors are not supported yet")
            }
            InputTensor::FloatTensor(_) => f32::unwrap_tensor_array(&array),
            InputTensor::DoubleTensor(_) => f64::unwrap_tensor_array(&array),
            InputTensor::StringTensor(_) => String::unwrap_tensor_array(&array),
        }
        .map_err(|e| anyhow!("failed to concatenate the tensors: {e}"))
    }
}

#[async_trait]
impl<T> ToTensor for BTreeMap<String, T>
where
    T: ToTensor<Field = TensorField, Output = InputTensor>,
{
    type Field = TensorFieldMap;
    type Output = Vec<InputTensor>;

    async fn into_tensor(
        self,
        fields: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        try_join_all(self.into_iter().map(|(name, item)| async move {
            let field = match fields.get(&name) {
                Some(field) => field,
                None => bail!("failed to find the field: {name:?}"),
            };

            item.into_tensor(field)
                .map_ok(|item| (field.index, item))
                .map_err(|e| anyhow!("failed to parse the tensor {name:?}: {e}"))
                .await
        }))
        .map_ok(|array| {
            array
                .into_iter()
                .sorted_by_key(|(index, _)| *index)
                .map(|(_, item)| item)
                .collect()
        })
        .await
    }
}

pub type TensorMap = BTreeMap<String, InputTensor>;

pub type TensorFieldMap = BTreeMap<String, TensorField>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorField {
    pub index: usize,
    pub kind: TensorKind,
    pub tensor_type: TensorType,
}

impl TensorField {
    pub fn try_from_input(index: usize, value: &Input) -> Result<Self> {
        Self::try_from_ort(index, &value.name, &value.dimensions, value.input_type)
    }

    pub fn try_from_output(index: usize, value: &Output) -> Result<Self> {
        Self::try_from_ort(index, &value.name, &value.dimensions, value.output_type)
    }

    fn try_from_ort(
        index: usize,
        name: &str,
        dimensions: &[Option<u32>],
        type_: TensorElementDataType,
    ) -> Result<Self> {
        let fail = || bail!("unsupported kind: {name:?}");

        match dimensions.len() {
            2 => match type_ {
                TensorElementDataType::Int64
                | TensorElementDataType::Float32
                | TensorElementDataType::Float64 => Ok(Self {
                    index,
                    kind: TensorKind::Text(TextKind {
                        max_len: dimensions[1],
                    }),
                    tensor_type: type_.into(),
                }),
                _ => fail(),
            },
            4 => match type_ {
                TensorElementDataType::Uint8 | TensorElementDataType::Float32 => {
                    // NCHW format
                    Ok(Self {
                        index,
                        kind: TensorKind::Image(ImageKind {
                            channels: dimensions[1].try_into()?, // C
                            width: dimensions[3],                // W
                            height: dimensions[2],               // H
                        }),
                        tensor_type: type_.into(),
                    })
                }
                _ => fail(),
            },
            _ => fail(),
        }
    }
}

impl TensorField {
    fn convert_bytes(&self, bytes: &[u8]) -> Result<InputTensor> {
        self.kind.convert_bytes(bytes, self.tensor_type)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "spec")]
pub enum TensorKind {
    Text(#[serde(default)] TextKind),
    Image(#[serde(default)] ImageKind),
}

impl TensorKind {
    fn convert_bytes(&self, bytes: &[u8], tensor_type: TensorType) -> Result<InputTensor> {
        match self {
            Self::Text(kind) => kind.convert_bytes(bytes),
            Self::Image(kind) => kind.convert_bytes(bytes, tensor_type),
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
    max_len: Option<u32>,
}

impl TextKind {
    fn convert_bytes(&self, bytes: &[u8]) -> Result<InputTensor> {
        String::from_utf8(bytes.to_vec())
            .map_err(Into::into)
            .and_then(|s| self.convert_string(s))
    }

    fn convert_string(&self, s: String) -> Result<InputTensor> {
        if let Some(max_len) = self.max_len {
            let len = s.len();
            if len > max_len as usize {
                bail!("too long string; expected <={max_len}, but given {len:?}");
            }
        }

        Ok(InputTensor::StringTensor(
            Array1::from_vec(vec![s]).into_dyn(),
        ))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageKind {
    channels: ImageChannel,
    width: Option<u32>,
    height: Option<u32>,
}

impl ImageKind {
    fn convert_bytes(&self, bytes: &[u8], tensor_type: TensorType) -> Result<InputTensor> {
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

trait UnwrapTensor {
    fn unwrap_tensor(tensor: &InputTensor) -> Result<ArrayView<Self, IxDyn>>
    where
        Self: Sized;

    fn unwrap_tensor_array(array: &[InputTensor]) -> Result<InputTensor>
    where
        Self: Sized;
}

macro_rules! impl_unwrap_tensor {
    ($name:ident => $ty:ty) => {
        impl UnwrapTensor for $ty {
            fn unwrap_tensor(tensor: &InputTensor) -> Result<ArrayView<Self, IxDyn>> {
                match tensor {
                    InputTensor::$name(tensor) => Ok(tensor.view()),
                    _ => bail!("cannot combine other types than {}", stringify!($ty)),
                }
            }

            fn unwrap_tensor_array(array: &[InputTensor]) -> Result<InputTensor> {
                let array: Vec<_> = array
                    .iter()
                    .map(<$ty as UnwrapTensor>::unwrap_tensor)
                    .collect::<Result<_>>()?;

                ndarray::concatenate(Axis(0), &array)
                    .map(InputTensor::$name)
                    .map_err(Into::into)
            }
        }
    };
}

impl_unwrap_tensor!(Int8Tensor => i8);
impl_unwrap_tensor!(Int16Tensor => i16);
impl_unwrap_tensor!(Int32Tensor => i32);
impl_unwrap_tensor!(Int64Tensor => i64);
impl_unwrap_tensor!(Uint8Tensor => u8);
impl_unwrap_tensor!(Uint16Tensor => u16);
impl_unwrap_tensor!(Uint32Tensor => u32);
impl_unwrap_tensor!(Uint64Tensor => u64);
impl_unwrap_tensor!(FloatTensor => f32);
impl_unwrap_tensor!(DoubleTensor => f64);
impl_unwrap_tensor!(StringTensor => String);
