use crate::{connector::NetworkConnectorDB, function::NetworkFunctionDB};

pub trait NetworkResourceDB
where
    Self: NetworkConnectorDB + NetworkFunctionDB,
{
}

impl<T> NetworkResourceDB for T where Self: NetworkConnectorDB + NetworkFunctionDB {}
