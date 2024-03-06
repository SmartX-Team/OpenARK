pub mod decoder;
pub mod encoder;

use std::io::Cursor;

use anyhow::{anyhow, Result};
use arrow::{
    datatypes::{DataType, Schema},
    json::reader::infer_json_schema_from_seekable,
};
use schemars::schema::RootSchema;
use serde::Serialize;

pub trait ToArrowSchema {
    fn to_arrow_schema(&self) -> Result<Schema>;
}

impl ToArrowSchema for RootSchema {
    fn to_arrow_schema(&self) -> Result<Schema> {
        todo!()
    }
}

impl<T> ToArrowSchema for [T]
where
    T: Serialize,
{
    fn to_arrow_schema(&self) -> Result<Schema> {
        let reader = Cursor::new(
            self.iter()
                .filter_map(|value| ::serde_json::to_vec(value).ok())
                .take(1)
                .flatten()
                .collect::<Vec<_>>(),
        );
        let (schema, _) = infer_json_schema_from_seekable(reader, None)
            .map_err(|error| anyhow!("failed to infer object metadata schema: {error}"))?;
        Ok(schema)
    }
}

impl<T> ToArrowSchema for Vec<T>
where
    T: Serialize,
{
    fn to_arrow_schema(&self) -> Result<Schema> {
        <[T] as ToArrowSchema>::to_arrow_schema(self.as_slice())
    }
}

pub trait ToDataType {
    fn to_data_type(&self) -> Result<DataType>;
}

impl ToDataType for Schema {
    fn to_data_type(&self) -> Result<DataType> {
        Ok(DataType::Struct(self.fields().clone()))
    }
}
