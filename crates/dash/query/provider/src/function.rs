use std::{fmt, sync::Arc};

use anyhow::Result;
use dash_api::function::{FunctionSpec, FunctionVolatility};
use dash_pipe_provider::{
    deltalake::{
        arrow::datatypes::{DataType, Schema},
        datafusion::{
            error::DataFusionError,
            logical_expr::{ScalarUDFImpl, Signature, TypeSignature, Volatility},
            physical_plan::ColumnarValue,
        },
    },
    messengers::Messenger,
    DeltaFunction, GenericStatelessRemoteFunction, Name,
};
use derivative::Derivative;
use futures::executor::block_on;
use inflector::Inflector;
use tokio::{spawn, sync::oneshot};
use tracing::{instrument, Level};

#[derive(Derivative)]
#[derivative(Debug)]
pub(crate) struct DashFunction {
    #[derivative(Debug = "ignore")]
    function: DeltaFunction,
    name: Name,
    name_udf: String,
    output: DataType,
    signature: Signature,
    spec: RemoteFunctionSpec,
}

impl DashFunction {
    #[instrument(level = Level::INFO, skip(messenger, spec), err(Display))]
    pub(crate) async fn try_new(
        messenger: &dyn Messenger,
        name: Name,
        model_in: Name,
        spec: RemoteFunctionSpec,
    ) -> Result<Self> {
        let RemoteFunctionSpec {
            input: input_schema,
            output: output_schema,
            exec: _,
            type_: _,
            volatility,
        } = &spec;

        let name_udf = name.to_snake_case();
        let input = DataType::Struct(input_schema.fields().clone());
        let inputs = input_schema
            .fields()
            .iter()
            .map(|field| field.data_type().clone())
            .collect();
        let output = DataType::Struct(output_schema.fields().clone());

        let function = GenericStatelessRemoteFunction::try_new(messenger, model_in)
            .await?
            .into_delta(input_schema.clone(), output_schema.clone());

        #[instrument(level = Level::INFO, skip(function, inputs), err(Display))]
        fn wrap_function(
            info: &str,
            function: &DeltaFunction,
            inputs: &[ColumnarValue],
        ) -> Result<ColumnarValue, DataFusionError> {
            let (tx, rx) = oneshot::channel();

            let function = function.clone();
            let inputs = inputs.to_vec();
            spawn(async move {
                let result = function.call(&inputs).await;
                let _ = tx.send(result);
            });

            block_on(rx)
                .map_err(|error| DataFusionError::External(error.into()))
                .and_then(|result| result.map_err(|error| DataFusionError::External(error.into())))
        }

        Ok(Self {
            function,
            name,
            name_udf,
            output,
            signature: Signature {
                type_signature: TypeSignature::OneOf(vec![
                    TypeSignature::Exact(vec![input]),
                    TypeSignature::Exact(inputs),
                ]),
                volatility: match volatility {
                    FunctionVolatility::Immutable => Volatility::Immutable,
                    FunctionVolatility::Stable => Volatility::Stable,
                    FunctionVolatility::Volatile => Volatility::Volatile,
                },
            },
            spec,
        })
    }
}

impl ScalarUDFImpl for DashFunction {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn name(&self) -> &str {
        &self.name_udf
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(
        &self,
        _arg_types: &[DataType],
    ) -> ::dash_pipe_provider::deltalake::datafusion::error::Result<DataType> {
        Ok(self.output.clone())
    }

    #[instrument(level = Level::INFO, skip(self, args), err(Display))]
    fn invoke(
        &self,
        args: &[ColumnarValue],
    ) -> ::dash_pipe_provider::deltalake::datafusion::error::Result<ColumnarValue> {
        let (tx, rx) = oneshot::channel();

        let function = self.function.clone();
        let inputs = args.to_vec();
        spawn(async move {
            let result = function.call(&inputs).await;
            let _ = tx.send(result);
        });

        block_on(rx)
            .map_err(|error| DataFusionError::External(error.into()))
            .and_then(|result| result.map_err(|error| DataFusionError::External(error.into())))
    }
}

impl fmt::Display for DashFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            name,
            spec:
                FunctionSpec {
                    input,
                    output: _,
                    exec: _,
                    type_: _,
                    volatility: _,
                },
            ..
        } = self;

        write!(f, "{name}(")?;
        for (index, name) in input.fields().iter().map(|field| field.name()).enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            name.fmt(f)?;
        }
        write!(f, ")")
    }
}

type RemoteFunctionSpec = FunctionSpec<Arc<Schema>, ()>;
