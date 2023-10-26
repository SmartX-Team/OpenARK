use std::sync::Arc;

use anyhow::Result;
use dash_api::model::ModelFieldsNativeSpec;
use dash_pipe_provider::{
    deltalake::{
        arrow::datatypes::{DataType, Field},
        datafusion::{
            error::DataFusionError,
            logical_expr::{ScalarUDF, Signature, TypeSignature, Volatility},
            physical_plan::ColumnarValue,
        },
    },
    storage::lakehouse::schema::{FieldColumns, ToField},
    Name,
};
use inflector::Inflector;
use kube::Client;

#[derive(Debug)]
pub struct DashFunction {
    name: Name,
    input: DataType,
    output: DataType,
}

impl DashFunction {
    pub fn try_new(
        _kube: Client,
        name: Name,
        input: &ModelFieldsNativeSpec,
        output: &ModelFieldsNativeSpec,
    ) -> Result<Self> {
        fn to_data_type(spec: &ModelFieldsNativeSpec) -> Result<DataType> {
            spec.to_data_columns()?
                .into_iter()
                .map(|schema| schema.to_field())
                .collect::<Result<_>>()
                .map(DataType::Struct)
        }

        Ok(Self {
            name,
            input: to_data_type(input)?,
            output: to_data_type(output)?,
        })
    }
}

impl From<DashFunction> for ScalarUDF {
    fn from(value: DashFunction) -> Self {
        Self {
            name: value.name.to_snake_case(),
            signature: Signature {
                type_signature: TypeSignature::Exact(vec![value.input]),
                volatility: Volatility::Immutable,
            },
            return_type: {
                let return_type = Arc::new(value.output);
                Arc::new(move |_| Ok(Arc::clone(&return_type)))
            },
            fun: Arc::new(execute),
        }
    }
}

fn execute(args: &[ColumnarValue]) -> Result<ColumnarValue, DataFusionError> {
    println!("{args:#?}");
    // todo!()
    Ok(ColumnarValue::Array(Arc::new(
        ::dash_pipe_provider::deltalake::arrow::array::StructArray::new(
            args.iter()
                .enumerate()
                .map(|(index, arg)| Field::new(format!("arg{index}"), arg.data_type(), false))
                .collect::<Vec<_>>()
                .into(),
            args.iter().map(|arg| arg.clone().into_array(1)).collect(),
            None,
        ),
    )))
}
