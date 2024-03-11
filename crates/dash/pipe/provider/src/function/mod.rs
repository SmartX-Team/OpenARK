pub mod connector;

use std::{fmt, marker::PhantomData, ops, sync::Arc};

use anyhow::{bail, Error, Result};
use ark_core::signal::FunctionSignal;
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use clap::{ArgMatches, Args, Command, FromArgMatches};
use futures::{stream::FuturesOrdered, TryStreamExt};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{instrument, Level};

use crate::{
    message::{PipeMessage, PipeMessages},
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
    use tracing::{instrument, Level};

    use super::{GenericStatelessRemoteFunction, PipeMessages, RemoteFunction};

    #[derive(Clone)]
    pub struct DeltaFunction {
        pub(super) chunk_size: usize,
        pub(super) inner: GenericStatelessRemoteFunction,
        pub(super) input: Arc<Schema>,
        pub(super) output: Arc<Schema>,
    }

    impl DeltaFunction {
        #[instrument(level = Level::INFO, skip_all, err(Display))]
        pub async fn call(
            &self,
            inputs: &[ColumnarValue],
        ) -> Result<ColumnarValue, DataFusionError> {
            let inputs = match inputs.len() {
                1 => inputs[0].clone().into_array(1)?,
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
                        .collect::<Result<_, _>>()?;

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

#[derive(Clone)]
pub struct OwnedFunctionBuilder<F>(Arc<F>);

impl<F> fmt::Debug for OwnedFunctionBuilder<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OwnedFunctionBuilder").finish()
    }
}

#[async_trait]
impl<F> FunctionBuilder for OwnedFunctionBuilder<F>
where
    F: Send + Sync + RemoteFunction,
    <F as RemoteFunction>::Input: fmt::Debug + DeserializeOwned + JsonSchema,
    <F as RemoteFunction>::Output: fmt::Debug + Serialize + JsonSchema,
{
    type Args = OwnedFunctionBuilderArgs<F>;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_new(
        OwnedFunctionBuilderArgs(f): &<Self as FunctionBuilder>::Args,
        _ctx: &mut FunctionContext,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        match f {
            Some(f) => Ok(Self(f.clone())),
            None => bail!("cannot create empty owned function"),
        }
    }
}

#[derive(Clone)]
pub struct OwnedFunctionBuilderArgs<F>(Option<Arc<F>>);

impl<F> OwnedFunctionBuilderArgs<F> {
    pub(crate) fn new(function: F) -> Self {
        Self(Some(Arc::new(function)))
    }
}

impl<F> FromArgMatches for OwnedFunctionBuilderArgs<F> {
    fn from_arg_matches(_matches: &ArgMatches) -> Result<Self, ::clap::Error> {
        Ok(Self(None))
    }

    fn update_from_arg_matches(&mut self, _matches: &ArgMatches) -> Result<(), ::clap::Error> {
        Ok(())
    }
}

impl<F> Args for OwnedFunctionBuilderArgs<F> {
    fn augment_args(cmd: Command) -> Command {
        cmd
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        cmd
    }
}

#[async_trait]
impl<F> RemoteFunction for OwnedFunctionBuilder<F>
where
    F: RemoteFunction,
{
    type Input = <F as RemoteFunction>::Input;
    type Output = <F as RemoteFunction>::Output;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output>> {
        self.0.call(inputs).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output>> {
        self.0.call_one(input).await
    }
}

#[async_trait]
pub trait RemoteFunction
where
    Self: Send + Sync,
{
    type Input: 'static + Send + Sync + Serialize;
    type Output: 'static + Send + Sync + DeserializeOwned;

    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output>> {
        inputs
            .into_vec()
            .into_iter()
            .map(|input| async move { self.call_one(input).await })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
            .map(PipeMessages::Batch)
    }

    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output>>;
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
    #[instrument(level = Level::INFO, skip(messenger), err(Display))]
    pub async fn try_new<M>(messenger: M, model_in: Name) -> Result<Self>
    where
        M: Messenger<Output>,
        Output: Send + DeserializeOwned,
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
    Input: 'static + Send + Sync + Serialize,
    Output: 'static + Send + Sync + DeserializeOwned,
{
    type Input = Input;
    type Output = Output;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output>> {
        inputs
            .into_vec()
            .into_iter()
            .map(|input| {
                let publisher = self.publisher.clone();
                async move {
                    publisher
                        .request_one((&input).try_into()?)
                        .await
                        .and_then(|outputs| outputs.try_into())
                }
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
            .map(|outputs| PipeMessages::Batch(outputs))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output>> {
        let publisher = self.publisher.clone();
        publisher
            .request_one((&input).try_into()?)
            .await
            .and_then(|outputs| outputs.try_into())
    }
}

#[async_trait]
pub trait FunctionBuilder
where
    Self: fmt::Debug + Function,
{
    type Args: Send + Args;

    async fn try_new(
        args: &<Self as FunctionBuilder>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self>
    where
        Self: Sized;
}

#[async_trait]
impl<T> Function for T
where
    T: RemoteFunction,
    <T as RemoteFunction>::Input: fmt::Debug + DeserializeOwned + JsonSchema,
    <T as RemoteFunction>::Output: fmt::Debug + Serialize + JsonSchema,
{
    type Input = <T as RemoteFunction>::Input;
    type Output = <T as RemoteFunction>::Output;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as Function>::Input>,
    ) -> Result<PipeMessages<<Self as Function>::Output>> {
        self.call(inputs.drop_payloads())
            .await
            .map(|outputs| outputs.drop_payloads())
    }
}

#[async_trait]
pub trait Function {
    type Input: 'static + Send + Sync + fmt::Debug + DeserializeOwned + JsonSchema;
    type Output: 'static + Send + Sync + fmt::Debug + Serialize + JsonSchema;

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
    messenger_type: MessengerType,
    signal: FunctionSignal,
}

impl ops::Deref for FunctionContext {
    type Target = FunctionSignal;

    fn deref(&self) -> &Self::Target {
        &self.signal
    }
}

impl FunctionContext {
    pub(crate) fn new(messenger_type: MessengerType) -> Self {
        Self {
            is_disabled_load: Default::default(),
            is_disabled_store: Default::default(),
            is_disabled_store_metadata: Default::default(),
            messenger_type,
            signal: Default::default(),
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

pub trait FunctionSignalExt {
    fn terminate_ok<T>(&self) -> Result<PipeMessages<T>>;

    fn terminate_err<T>(&self, error: impl Into<Error>) -> Result<PipeMessages<T>>;
}

impl FunctionSignalExt for FunctionSignal {
    fn terminate_ok<T>(&self) -> Result<PipeMessages<T>> {
        self.terminate();
        Ok(PipeMessages::None)
    }

    fn terminate_err<T>(&self, error: impl Into<Error>) -> Result<PipeMessages<T>> {
        self.terminate();
        Err(error.into())
    }
}
