use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
};

use anyhow::{anyhow, bail, Error, Result};
use async_trait::async_trait;
use byteorder::{ByteOrder, NetworkEndian};
use bytes::Bytes;
use futures::{future::try_join_all, TryFutureExt};
use half::{bf16, f16};
use image::{imageops::FilterType, GenericImageView, Pixel};
use itertools::Itertools;
use ndarray::{Array, Array1, ArrayBase, ArrayView, Axis, IxDyn, IxDynImpl};
use ort::{
    session::{Input, Output},
    tensor::{OrtOwnedTensor, TensorElementDataType},
    value::DynArrayRef,
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::io::AsyncReadExt;
use tracing::warn;

use crate::primitive::AsPrimitive;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchedTensor<Value = Bytes>
where
    Value: Default,
{
    batches: Vec<TensorSet<Value>>,
}

impl BatchedTensor {
    pub fn collect_column<'a, 'k>(&'a self, key: &'k str) -> Result<DynArrayRef<'static>>
    where
        'a: 'k,
    {
        let tensors: Vec<_> = self
            .collect_column_iter(key)
            .map(DynArrayRef::try_from)
            .collect::<Result<_>>()
            .map_err(|error| anyhow!("failed to convert tensor type: {error}"))?;

        match &tensors[0] {
            DynArrayRef::Bool(_) => bool::unwrap_tensor_array(&tensors),
            DynArrayRef::Int8(_) => i8::unwrap_tensor_array(&tensors),
            DynArrayRef::Int16(_) => i16::unwrap_tensor_array(&tensors),
            DynArrayRef::Int32(_) => i32::unwrap_tensor_array(&tensors),
            DynArrayRef::Int64(_) => i64::unwrap_tensor_array(&tensors),
            DynArrayRef::Uint8(_) => u8::unwrap_tensor_array(&tensors),
            DynArrayRef::Uint16(_) => u16::unwrap_tensor_array(&tensors),
            DynArrayRef::Uint32(_) => u32::unwrap_tensor_array(&tensors),
            DynArrayRef::Uint64(_) => u64::unwrap_tensor_array(&tensors),
            DynArrayRef::Bfloat16(_) => bf16::unwrap_tensor_array(&tensors),
            DynArrayRef::Float16(_) => f16::unwrap_tensor_array(&tensors),
            DynArrayRef::Float(_) => f32::unwrap_tensor_array(&tensors),
            DynArrayRef::Double(_) => f64::unwrap_tensor_array(&tensors),
            DynArrayRef::String(_) => String::unwrap_tensor_array(&tensors),
        }
    }
}

impl<Value> BatchedTensor<Value>
where
    Value: Default,
{
    fn collect_column_iter<'a, 'k>(
        &'a self,
        key: &'k str,
    ) -> impl 'k + Iterator<Item = &'a Tensor<Value>>
    where
        'a: 'k,
    {
        self.batches.iter().filter_map(|batch| batch.map.get(key))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorSet<Value = Bytes>
where
    Value: Default,
{
    map: HashMap<String, Tensor<Value>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tensor<Value = Bytes> {
    metadata: TensorMetadata,
    #[serde(default)]
    value: Value,
}

impl<'v> TryFrom<&'v Tensor> for DynArrayRef<'v> {
    type Error = Error;

    fn try_from(value: &'v Tensor) -> Result<Self> {
        let Tensor {
            metadata: TensorMetadata { dim, type_ },
            value,
        } = value;
        let shape: IxDynImpl = dim.as_slice().into();

        match type_ {
            TensorType::Bool => {
                let buffer = value.iter().map(|&e| e != 0).collect();
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Bool)
            }
            TensorType::Int8 => {
                let buffer = value.iter().map(|&e| e as i8).collect();
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Int8)
            }
            TensorType::Int16 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<i16>());
                NetworkEndian::read_i16_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Int16)
            }
            TensorType::Int32 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<i32>());
                NetworkEndian::read_i32_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Int32)
            }
            TensorType::Int64 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<i64>());
                NetworkEndian::read_i64_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Int64)
            }
            TensorType::Uint8 => ArrayView::from_shape(shape, value)
                .map(Into::into)
                .map(Self::Uint8),
            TensorType::Uint16 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<u16>());
                NetworkEndian::read_u16_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Uint16)
            }
            TensorType::Uint32 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<u32>());
                NetworkEndian::read_u32_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Uint32)
            }
            TensorType::Uint64 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<u64>());
                NetworkEndian::read_u64_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Uint64)
            }
            TensorType::Bfloat16 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<bf16>());
                NetworkEndian::read_u16_into(value, &mut buffer);
                let buffer = buffer.into_iter().map(bf16::from_bits).collect();
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Bfloat16)
            }
            TensorType::Float16 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<f16>());
                NetworkEndian::read_u16_into(value, &mut buffer);
                let buffer = buffer.into_iter().map(f16::from_bits).collect();
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Float16)
            }
            TensorType::Float32 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<f32>());
                NetworkEndian::read_f32_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Float)
            }
            TensorType::Float64 => {
                let mut buffer = Vec::with_capacity(value.len() / size_of::<f64>());
                NetworkEndian::read_f64_into(value, &mut buffer);
                Array::from_shape_vec(shape, buffer)
                    .map(Into::into)
                    .map(Self::Double)
            }
            // TODO: to be implemented!
            TensorType::String => todo!(),
        }
        .map_err(Into::into)
    }
}

impl<'v> From<DynArrayRef<'v>> for Tensor {
    fn from(value: DynArrayRef<'v>) -> Self {
        match value {
            DynArrayRef::Bool(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Bool,
                },
                value: array
                    .into_iter()
                    .map(|e| e as u8)
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Int8(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Bool,
                },
                value: array
                    .into_iter()
                    .map(|e| e as u8)
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Int16(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Int16,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<i16>()];
                        NetworkEndian::write_i16(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Int32(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Int32,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<i32>()];
                        NetworkEndian::write_i32(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Int64(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Int64,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<i64>()];
                        NetworkEndian::write_i64(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Uint8(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Uint8,
                },
                value: array.into_iter().collect::<Vec<_>>().into(),
            },
            DynArrayRef::Uint16(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Uint16,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<u16>()];
                        NetworkEndian::write_u16(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Uint32(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Uint32,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<u32>()];
                        NetworkEndian::write_u32(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Uint64(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Uint64,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<u64>()];
                        NetworkEndian::write_u64(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Bfloat16(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Bfloat16,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<bf16>()];
                        NetworkEndian::write_u16(&mut buf, e.to_bits());
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Float16(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Float16,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<f16>()];
                        NetworkEndian::write_u16(&mut buf, e.to_bits());
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Float(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Float32,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<f32>()];
                        NetworkEndian::write_f32(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            DynArrayRef::Double(array) => Self {
                metadata: TensorMetadata {
                    dim: array.shape().to_vec(),
                    type_: TensorType::Float64,
                },
                value: array
                    .into_iter()
                    .flat_map(|e| {
                        let mut buf = [0; size_of::<f64>()];
                        NetworkEndian::write_f64(&mut buf, e);
                        buf
                    })
                    .collect::<Vec<_>>()
                    .into(),
            },
            // TODO: to be implemented!
            DynArrayRef::String(_array) => todo!(),
        }
    }
}

impl<Value> Tensor<Value> {
    pub fn detach(self) -> (Tensor<()>, Value) {
        let Self { metadata, value } = self;
        let tensor = Tensor {
            metadata,
            value: (),
        };
        (tensor, value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorMetadata {
    dim: Vec<usize>,
    #[serde(rename = "type")]
    type_: TensorType,
}

#[allow(clippy::enum_variant_names)]
pub enum OutputTensor<'a, D = IxDyn>
where
    D: ::ndarray::Dimension,
{
    Bool(OrtOwnedTensor<'a, bool, D>),
    Int8(OrtOwnedTensor<'a, i8, D>),
    Int16(OrtOwnedTensor<'a, i16, D>),
    Int32(OrtOwnedTensor<'a, i32, D>),
    Int64(OrtOwnedTensor<'a, i64, D>),
    Uint8(OrtOwnedTensor<'a, u8, D>),
    Uint16(OrtOwnedTensor<'a, u16, D>),
    Uint32(OrtOwnedTensor<'a, u32, D>),
    Uint64(OrtOwnedTensor<'a, u64, D>),
    Bfloat16(OrtOwnedTensor<'a, ::half::bf16, D>),
    Float16(OrtOwnedTensor<'a, ::half::f16, D>),
    Float(OrtOwnedTensor<'a, f32, D>),
    Double(OrtOwnedTensor<'a, f64, D>),
    String(OrtOwnedTensor<'a, String, D>),
}

impl<'a, D> OutputTensor<'a, D>
where
    D: 'a + ::ndarray::Dimension,
{
    fn argmax_with<S>(mat: &ArrayBase<S, D>) -> Array1<usize>
    where
        S: ::ndarray::Data,
        S::Elem: PartialOrd,
    {
        mat.rows()
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                    .unwrap()
                    .0
            })
            .collect()
    }

    fn argmax_by_group_with<S>(
        mat: &ArrayBase<S, D>,
        mut label: usize,
        label_drop: Option<usize>,
        groups: &[usize],
    ) -> Array1<Option<usize>>
    where
        S: ::ndarray::Data,
        S::Elem: PartialOrd + AsPrimitive<f64> + ::std::fmt::Debug,
        D: ::ndarray::RemoveAxis,
        <D as ::ndarray::Dimension>::Smaller: ::ndarray::Dimension<Larger = D>,
    {
        let mat = match label_drop {
            Some(label_drop) => {
                let mut mat = mat.to_owned();
                {
                    mat.remove_index(Axis(1), label_drop);
                    if label_drop < label {
                        label -= 1;
                    }
                }
                Self::softmax_2d_with(&mat)
            }
            None => Self::softmax_2d_with(mat),
        };

        let mut max = mat.rows().into_iter().map(|row| {
            row.into_iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap()
        });

        groups
            .iter()
            .map(|&group| {
                max.by_ref()
                    .take(group)
                    .enumerate()
                    .filter(|(_, (group_label, _))| *group_label == label)
                    .map(|(group_index, (_, value))| (group_index, value))
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                    .map(|(group_index, _)| group_index)
            })
            .collect()
    }

    fn softmax_with<S>(mat: &ArrayBase<S, D>, axis: Axis) -> Array<f64, D>
    where
        S: ::ndarray::Data,
        S::Elem: PartialOrd + AsPrimitive<f64>,
        D: ::ndarray::RemoveAxis,
        <D as ::ndarray::Dimension>::Smaller: ::ndarray::Dimension<Larger = D>,
    {
        let exp = mat.mapv(|value| value.as_().exp());
        let sum = exp.sum_axis(axis).insert_axis(axis);
        exp / sum
    }

    fn softmax_2d_with<S>(mat: &ArrayBase<S, D>) -> Array<f64, D>
    where
        S: ::ndarray::Data,
        S::Elem: PartialOrd + AsPrimitive<f64>,
        D: ::ndarray::RemoveAxis,
        <D as ::ndarray::Dimension>::Smaller: ::ndarray::Dimension<Larger = D>,
    {
        Self::softmax_with(mat, Axis(1))
    }
}

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
    type Output = DynArrayRef<'static>;

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
    type Output = DynArrayRef<'static>;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        let mut file = ::tokio::fs::File::from_std(self.file.into_file());
        let mut buf = Default::default();
        file.read_to_end(&mut buf).await?;

        field.convert_bytes(&buf)
    }
}

#[async_trait]
impl ToTensor for ::actix_multipart::form::text::Text<String> {
    type Field = TensorField;
    type Output = DynArrayRef<'static>;

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
    T: ToTensor<Field = TensorField, Output = DynArrayRef<'static>>,
{
    type Field = TensorField;
    type Output = DynArrayRef<'static>;

    async fn into_tensor(
        self,
        field: &<Self as ToTensor>::Field,
    ) -> Result<<Self as ToTensor>::Output> {
        let array = try_join_all(self.into_iter().map(|item| item.into_tensor(field))).await?;

        if array.is_empty() {
            bail!("failed to parse zero-sized tensor");
        }

        match &array[0] {
            DynArrayRef::Bool(_) => bool::unwrap_tensor_array(&array),
            DynArrayRef::Int8(_) => i8::unwrap_tensor_array(&array),
            DynArrayRef::Int16(_) => i16::unwrap_tensor_array(&array),
            DynArrayRef::Int32(_) => i32::unwrap_tensor_array(&array),
            DynArrayRef::Int64(_) => i64::unwrap_tensor_array(&array),
            DynArrayRef::Uint8(_) => u8::unwrap_tensor_array(&array),
            DynArrayRef::Uint16(_) => u16::unwrap_tensor_array(&array),
            DynArrayRef::Uint32(_) => u32::unwrap_tensor_array(&array),
            DynArrayRef::Uint64(_) => u64::unwrap_tensor_array(&array),
            DynArrayRef::Bfloat16(_) => {
                bail!("concatenating Bfloat16Tensors are not supported yet")
            }
            DynArrayRef::Float16(_) => {
                bail!("concatenating Float16Tensors are not supported yet")
            }
            DynArrayRef::Float(_) => f32::unwrap_tensor_array(&array),
            DynArrayRef::Double(_) => f64::unwrap_tensor_array(&array),
            DynArrayRef::String(_) => String::unwrap_tensor_array(&array),
        }
        .map_err(|e| anyhow!("failed to concatenate the tensors: {e}"))
    }
}

#[async_trait]
impl<T> ToTensor for BTreeMap<String, T>
where
    T: ToTensor<Field = TensorField, Output = DynArrayRef<'static>>,
{
    type Field = TensorFieldMap;
    type Output = Vec<DynArrayRef<'static>>;

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

    pub fn try_from_output(index: usize, value: &Output) -> Option<Self> {
        match Self::try_from_ort(index, &value.name, &value.dimensions, value.output_type) {
            Ok(field) => Some(field),
            Err(e) => {
                warn!("error parsing OutputTensor: {e}");
                None
            }
        }
    }

    fn try_from_ort(
        index: usize,
        name: &str,
        dimensions: &[Option<u32>],
        type_: TensorElementDataType,
    ) -> Result<Self> {
        let fail = || {
            let dimensions = dimensions
                .iter()
                .map(|dimension| match dimension {
                    Some(dimension) => dimension.to_string(),
                    None => "*".into(),
                })
                .join(", ");
            bail!("unsupported tensor kind: {name:?} as {type_:?}[{dimensions}]")
        };

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
    fn convert_bytes(&self, bytes: &[u8]) -> Result<DynArrayRef<'static>> {
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
    fn convert_bytes(&self, bytes: &[u8], tensor_type: TensorType) -> Result<DynArrayRef<'static>> {
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
    fn convert_bytes(&self, bytes: &[u8]) -> Result<DynArrayRef<'static>> {
        String::from_utf8(bytes.to_vec())
            .map_err(Into::into)
            .and_then(|s| self.convert_string(s))
    }

    fn convert_string(&self, s: String) -> Result<DynArrayRef<'static>> {
        if let Some(max_len) = self.max_len {
            let len = s.len();
            if len > max_len as usize {
                bail!("too long string; expected <={max_len}, but given {len:?}");
            }
        }

        Ok(DynArrayRef::String(
            Array1::from_vec(vec![s]).into_dyn().into(),
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
    fn convert_bytes(&self, bytes: &[u8], tensor_type: TensorType) -> Result<DynArrayRef<'static>> {
        fn convert_image<I>(
            image: I,
            tensor_type: TensorType,
            shape: (usize, usize, usize, usize),
        ) -> DynArrayRef<'static>
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
                    DynArrayRef::Uint8(Array::from_shape_fn(shape, get_pixel).into_dyn().into())
                }
                TensorType::Float32 => DynArrayRef::Float(
                    Array::from_shape_fn(shape, |idx| (get_pixel(idx) as f32) / 255.0)
                        .into_dyn()
                        .into(),
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
    fn unwrap_tensor<'a>(tensor: &'a DynArrayRef) -> Result<ArrayView<'a, Self, IxDyn>>
    where
        Self: Sized;

    fn unwrap_tensor_array(array: &[DynArrayRef]) -> Result<DynArrayRef<'static>>
    where
        Self: Sized;
}

macro_rules! impl_tensor {
    ( $( $name:ident => $ty:ty , )* ) => {
        impl<'a, D> OutputTensor<'a, D>
        where
            D: 'a + ::ndarray::Dimension,
        {
            pub fn argmax(&self) -> Array1<usize> {
                match self {
                    $(
                        Self::$name(tensor) => Self::argmax_with(&tensor.view()),
                    )*
                }
            }

            pub fn argmax_by_group(
                &self,
                label: usize,
                label_drop: Option<usize>,
                groups: &[usize],
            ) -> Array1<Option<usize>>
            where
                D: ::ndarray::RemoveAxis,
                <D as ::ndarray::Dimension>::Smaller: ::ndarray::Dimension<Larger = D>,
            {
                match self {
                    $(
                        Self::$name(tensor) => Self::argmax_by_group_with(
                            &tensor.view(),
                            label,
                            label_drop,
                            groups,
                        ),
                    )*
                }
            }
        }

        $(
            impl UnwrapTensor for $ty {
                fn unwrap_tensor<'a>(tensor: &'a DynArrayRef) -> Result<ArrayView<'a, Self, IxDyn>> {
                    match tensor {
                        DynArrayRef::$name(tensor) => Ok(tensor.view()),
                        _ => bail!("cannot combine other types than {}", stringify!($ty)),
                    }
                }

                fn unwrap_tensor_array(array: &[DynArrayRef]) -> Result<DynArrayRef<'static>> {
                    let array: Vec<_> = array
                        .iter()
                        .map(<$ty as UnwrapTensor>::unwrap_tensor)
                        .collect::<Result<_>>()?;

                    let concatenated: ::ndarray::CowArray<Self, IxDyn> =
                        ::ndarray::concatenate(Axis(0), &array)?.into();
                    Ok(DynArrayRef::$name(concatenated))
                }
            }

            impl<'a, D> From<OrtOwnedTensor<'a, $ty, D>> for OutputTensor<'a, D>
            where
                D: ::ndarray::Dimension,
            {
                fn from(value: OrtOwnedTensor<'a, $ty, D>) -> Self {
                    Self::$name(value)
                }
            }
        )*
    };
}

impl_tensor!(
    Bool => bool,
    Int8 => i8,
    Int16 => i16,
    Int32 => i32,
    Int64 => i64,
    Uint8 => u8,
    Uint16 => u16,
    Uint32 => u32,
    Uint64 => u64,
    Float => f32,
    Double => f64,
    Bfloat16 => ::half::bf16,
    Float16 => ::half::f16,
    String => String,
);
