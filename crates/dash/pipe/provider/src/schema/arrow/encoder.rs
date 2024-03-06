use std::sync::Arc;

use anyhow::Result;
use arrow::{array::RecordBatch, datatypes::Schema, json::reader::ReaderBuilder};
use serde::Serialize;

pub trait TryIntoRecordBatch {
    fn try_into_record_batch(self, schema: Arc<Schema>) -> Result<Option<RecordBatch>>;
}

impl<T> TryIntoRecordBatch for &[T]
where
    T: Serialize,
{
    fn try_into_record_batch(self, schema: Arc<Schema>) -> Result<Option<RecordBatch>> {
        let mut decoder = ReaderBuilder::new(schema).build_decoder()?;
        decoder.serialize(self)?;
        decoder.flush().map_err(Into::into)
    }
}
