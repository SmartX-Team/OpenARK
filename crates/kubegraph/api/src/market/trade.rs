use serde::{Deserialize, Serialize};

use super::product::ProductSpec;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TradeTemplate<T = <ProductSpec as super::BaseModel>::Id> {
    pub r#pub: T,
    pub sub: T,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeState {
    Success,
}
