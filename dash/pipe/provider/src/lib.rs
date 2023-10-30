// Re-export deltalake crate
#[cfg(feature = "deltalake")]
pub extern crate deltalake;

mod function;
mod message;
pub mod messengers;
mod pipe;
pub mod storage;

pub use ark_core_k8s::data::Name;

#[cfg(feature = "deltalake")]
pub use self::function::deltalake::DeltaFunction;
pub use self::function::{
    Function, FunctionBuilder, FunctionContext, GenericStatelessRemoteFunction, RemoteFunction,
    StatelessRemoteFunction,
};
#[cfg(feature = "pyo3")]
pub use self::message::PyPipeMessage;
pub use self::message::{PipeMessage, PipeMessages, PipePayload};
pub use self::messengers::MessengerType;
pub use self::pipe::{DefaultModelIn, PipeArgs};
