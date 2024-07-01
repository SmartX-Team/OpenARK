use serde::{Deserialize, Serialize};

use super::product::ProductSpec;

pub type PriceHistogram = Vec<PriceItem>;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceItem {
    pub id: <ProductSpec as super::BaseModel>::Id,
    pub direction: Direction,
    pub cost: <ProductSpec as super::BaseModel>::Cost,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Pub,
    Sub,
}
