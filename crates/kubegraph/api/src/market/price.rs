use serde::{Deserialize, Serialize};

use super::product::ProductSpec;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PriceHistogram(pub Vec<PriceItem>);

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceItem {
    pub direction: Direction,
    pub cost: <ProductSpec as super::BaseModel>::Cost,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Pub,
    Sub,
}
