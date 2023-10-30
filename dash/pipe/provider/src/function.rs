use std::{
    fmt,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use clap::Args;
use futures::future::try_join_all;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use tracing::info;

use crate::{
    message::{Name, PipeMessages},
    messengers::{Messenger, MessengerType, Publisher},
    storage::StorageIO,
};

#[cfg(feature = "deltalake")]
pub mod deltalake {
    use std::sync::Arc;

    use anyhow::anyhow;
    use deltalake::{
        arrow::{
            array::{Array, StructArray},
            datatypes::Schema,
            json::{writer::array_to_json_array, ReaderBuilder},
        },
        datafusion::{error::DataFusionError, physical_plan::ColumnarValue},
    };

    use super::{GenericStatelessRemoteFunction, PipeMessages, RemoteFunction};

    #[derive(Clone)]
    pub struct DeltaFunction {
        pub(super) chunk_size: usize,
        pub(super) inner: GenericStatelessRemoteFunction,
        pub(super) input: Arc<Schema>,
        pub(super) output: Arc<Schema>,
    }

    impl DeltaFunction {
        pub async fn call(
            &self,
            inputs: &[ColumnarValue],
        ) -> Result<ColumnarValue, DataFusionError> {
            let inputs = match inputs.len() {
                1 => inputs[0].clone().into_array(1),
                _ => {
                    let num_rows = inputs
                        .iter()
                        .map(|input| match input {
                            ColumnarValue::Array(array) => array.len(),
                            ColumnarValue::Scalar(_) => 1,
                        })
                        .max()
                        .unwrap_or_default();
                    if num_rows == 0 {
                        return Err(DataFusionError::External(anyhow!("empty inputs").into()));
                    }

                    let arrays: Vec<_> = inputs
                        .iter()
                        .map(|input| input.clone().into_array(num_rows))
                        .collect();

                    Arc::new(StructArray::new(self.input.fields().clone(), arrays, None))
                }
            };
            let num_rows = inputs.len();

            let mut decoder = ReaderBuilder::new(self.output.clone()).build_decoder()?;

            for arrays in (0..inputs.len())
                .step_by(self.chunk_size)
                .map(|offset| inputs.slice(offset, self.chunk_size.min(num_rows - offset)))
            {
                let inputs = array_to_json_array(&arrays)?
                    .into_iter()
                    .map(::serde_json::from_value)
                    .collect::<Result<_, _>>()
                    .map(PipeMessages::Batch)
                    .map_err(|error| DataFusionError::External(error.into()))?;

                let outputs = self
                    .inner
                    .call(inputs)
                    .await
                    .map_err(|error| DataFusionError::External(error.into()))?
                    .into_vec();

                decoder.serialize(&outputs)?;
            }

            let decoded = decoder.flush()?.unwrap();
            Ok(ColumnarValue::Array(Arc::new(StructArray::from(decoded))))
        }
    }
}

#[async_trait]
pub trait RemoteFunction {
    type Input: 'static + Send + Sync + Default + Serialize;
    type Output: 'static + Send + Sync + Default + DeserializeOwned;

    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output, ()>>;
}

pub type GenericStatelessRemoteFunction =
    StatelessRemoteFunction<::serde_json::Value, ::serde_json::Value>;

#[derive(Clone)]
pub struct StatelessRemoteFunction<Input, Output> {
    _input: PhantomData<Input>,
    _output: PhantomData<Output>,
    publisher: Arc<dyn Publisher>,
}

impl<Input, Output> StatelessRemoteFunction<Input, Output> {
    pub async fn try_new(messenger: &dyn Messenger<Output>, model_in: Name) -> Result<Self>
    where
        Output: Send + Default + DeserializeOwned,
    {
        Ok(Self {
            _input: PhantomData,
            _output: PhantomData,
            publisher: messenger.publish(model_in).await?,
        })
    }
}

impl StatelessRemoteFunction<::serde_json::Value, ::serde_json::Value> {
    #[cfg(feature = "deltalake")]
    pub fn into_delta(
        self,
        input: Arc<::deltalake::arrow::datatypes::Schema>,
        output: Arc<::deltalake::arrow::datatypes::Schema>,
    ) -> self::deltalake::DeltaFunction {
        self::deltalake::DeltaFunction {
            chunk_size: 8,
            inner: self,
            input,
            output,
        }
    }
}

#[async_trait]
impl<Input, Output> RemoteFunction for StatelessRemoteFunction<Input, Output>
where
    Input: 'static + Send + Sync + Default + Serialize,
    Output: 'static + Send + Sync + Default + DeserializeOwned,
{
    type Input = Input;
    type Output = Output;

    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input, ()>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output, ()>> {
        try_join_all(inputs.into_vec().into_iter().map(|input| {
            let publisher = self.publisher.clone();
            async move {
                publisher
                    .request_one((&input).try_into()?)
                    .await
                    .and_then(|outputs| outputs.try_into())
            }
        }))
        .await
        .map(|outputs| PipeMessages::Batch(outputs))
    }
}

#[async_trait(?Send)]
pub trait FunctionBuilder
where
    Self: Function,
{
    type Args: Clone + fmt::Debug + Serialize + DeserializeOwned + Args;

    async fn try_new(
        args: &<Self as FunctionBuilder>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait(?Send)]
impl<T> Function for T
where
    T: RemoteFunction,
    <T as RemoteFunction>::Input: DeserializeOwned + JsonSchema,
    <T as RemoteFunction>::Output: Serialize + JsonSchema,
{
    type Input = <T as RemoteFunction>::Input;
    type Output = <T as RemoteFunction>::Output;

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output>> {
        self.call(inputs.load_payloads_as_empty())
            .await
            .map(|outputs| outputs.load_payloads_as_empty())
    }
}

#[async_trait(?Send)]
pub trait Function {
    type Input: 'static + Send + Sync + Default + DeserializeOwned + JsonSchema;
    type Output: 'static + Send + Sync + Default + Serialize + JsonSchema;

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as Function>::Input>,
    ) -> Result<PipeMessages<<Self as Function>::Output>>;
}

#[derive(Clone, Debug)]
pub struct FunctionContext {
    is_disabled_load: bool,
    is_disabled_store: bool,
    is_disabled_store_metadata: bool,
    is_terminating: Arc<AtomicBool>,
    messenger_type: MessengerType,
}

impl FunctionContext {
    pub(crate) fn new(messenger_type: MessengerType) -> Self {
        Self {
            is_disabled_load: Default::default(),
            is_disabled_store: Default::default(),
            is_disabled_store_metadata: Default::default(),
            is_terminating: Default::default(),
            messenger_type,
        }
    }

    pub fn disable_load(&mut self) {
        self.is_disabled_load = true;
    }

    pub(crate) const fn is_disabled_load(&self) -> bool {
        self.is_disabled_load
    }

    pub fn disable_store(&mut self) {
        self.is_disabled_store = true;
    }

    pub(crate) const fn is_disabled_store(&self) -> bool {
        self.is_disabled_store
    }

    pub fn disable_store_metadata(&mut self) {
        self.is_disabled_store_metadata = true;
    }

    pub(crate) const fn is_disabled_store_metadata(&self) -> bool {
        self.is_disabled_store_metadata
    }

    pub const fn messenger_type(&self) -> MessengerType {
        self.messenger_type
    }
}

impl FunctionContext {
    pub(crate) fn trap_on_sigint(self) -> Result<()> {
        ::ctrlc::set_handler(move || self.terminate())
            .map_err(|error| anyhow!("failed to set SIGINT handler: {error}"))
    }

    pub(crate) fn terminate(&self) {
        info!("Gracefully shutting down...");
        self.is_terminating.store(true, Ordering::SeqCst)
    }

    pub fn terminate_ok<T>(&self) -> Result<PipeMessages<T>>
    where
        T: Default,
    {
        self.terminate();
        Ok(PipeMessages::None)
    }

    pub fn terminate_err<T>(&self, error: impl Into<Error>) -> Result<PipeMessages<T>>
    where
        T: Default,
    {
        self.terminate();
        Err(error.into())
    }

    pub(crate) fn is_terminating(&self) -> bool {
        self.is_terminating.load(Ordering::SeqCst)
    }
}
