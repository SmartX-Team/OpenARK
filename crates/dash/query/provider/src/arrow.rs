use std::sync::Arc;

use dash_pipe_provider::deltalake::arrow::{
    array::{ArrayRef, AsArray, StructArray},
    datatypes::{Field, FieldRef},
    record_batch::RecordBatch,
};

pub(crate) trait IntoFlattened {
    fn into_flattened(self) -> Self;
}

impl IntoFlattened for RecordBatch {
    fn into_flattened(self) -> Self {
        StructArray::from(self).into_flattened().into()
    }
}

impl IntoFlattened for StructArray {
    fn into_flattened(self) -> Self {
        let mut flattened_fields = vec![];
        flatten_struct_array(&mut flattened_fields, "", Arc::new(self));
        StructArray::from(flattened_fields)
    }
}

fn flatten_struct_array(
    flattened_fields: &mut Vec<(FieldRef, ArrayRef)>,
    name: &str,
    array: ArrayRef,
) {
    match array.as_struct_opt() {
        Some(array) => {
            let (fields, arrays, _nulls) = array.clone().into_parts();
            fields.into_iter().zip(arrays).for_each(|(field, array)| {
                let name = if name.is_empty() {
                    field.name().into()
                } else {
                    format!("{name}.{}", field.name())
                };
                flatten_struct_array(flattened_fields, &name, array)
            })
        }
        None => {
            let field = Arc::new(Field::new(
                name,
                array.data_type().clone(),
                array.is_nullable(),
            ));
            flattened_fields.push((field, array));
        }
    }
}
