use anyhow::Error;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::product::ProductSpec;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionSpec<
    Pub = <ProductSpec as super::BaseModel>::Id,
    Sub = <ProductSpec as super::BaseModel>::Id,
> {
    pub template: TransactionTemplate<Pub, Sub>,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "pub")]
    pub pub_spec: TaskSpec,
    #[serde(rename = "sub")]
    pub sub_spec: TaskSpec,
}

impl super::BaseModel for TransactionSpec {
    type Id = <ProductSpec as super::BaseModel>::Id;
    type Cost = <ProductSpec as super::BaseModel>::Cost;
    type Count = <ProductSpec as super::BaseModel>::Count;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskSpec {
    pub timestamp: DateTime<Utc>,
    pub state: TaskState,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TaskState {
    Running,
    Completed,
    Failed,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionTemplate<
    Pub = <ProductSpec as super::BaseModel>::Id,
    Sub = <ProductSpec as super::BaseModel>::Id,
> {
    pub r#pub: Pub,
    pub sub: Sub,
    pub cost: <TransactionSpec as super::BaseModel>::Cost,
    pub count: <TransactionSpec as super::BaseModel>::Count,
}

#[derive(Debug, Error)]
pub enum TransactionError {
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
