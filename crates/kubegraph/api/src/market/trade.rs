use anyhow::Error;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::product::ProductSpec;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeTemplate<
    Pub = <ProductSpec as super::BaseModel>::Id,
    Sub = <ProductSpec as super::BaseModel>::Id,
> {
    pub r#pub: Pub,
    pub sub: Sub,
    pub count: <ProductSpec as super::BaseModel>::Count,
}

#[derive(Debug, Error)]
pub enum TradeError {
    #[error("requested count is zero or negative")]
    EmptyCount,
    #[error("the transaction is succeeded, but failed to call pub function: {0}")]
    FunctionFailedPub(Error),
    #[error("the transaction is succeeded, but failed to call sub function: {0}")]
    FunctionFailedSub(Error),
    #[error("this pub is out of stock")]
    OutOfPub,
    #[error("this sub is out of stock")]
    OutOfSub,
    #[error("internal transaction error")]
    TransactionFailed,
}
