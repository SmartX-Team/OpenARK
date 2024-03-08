#![recursion_limit = "256"]

// Re-export deltalake crate
#[cfg(feature = "deltalake")]
pub extern crate deltalake;
#[cfg(feature = "lancedb")]
pub extern crate lancedb;

mod client;
mod function;
mod message;
pub mod messengers;
mod pipe;
pub mod schema;
pub mod storage;

pub use ark_core_k8s::data::Name;

pub use self::client::{PipeClient, PipeClientArgs};
#[cfg(feature = "deltalake")]
pub use self::function::deltalake::DeltaFunction;
pub use self::function::{
    connector, Function, FunctionBuilder, FunctionContext, FunctionSignalExt,
    GenericStatelessRemoteFunction, OwnedFunctionBuilder, RemoteFunction, StatelessRemoteFunction,
};
#[cfg(feature = "pyo3")]
pub use self::message::PyPipeMessage;
pub use self::message::{
    Codec, DynMap, DynValue, MaybePipeMessage, PipeMessage, PipeMessages, PipePayload,
};
pub use self::messengers::MessengerType;
pub use self::pipe::{DefaultModelIn, PipeArgs};
