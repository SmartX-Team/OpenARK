use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::product::ProductSpec;

pub type PriceHistogram = Vec<PriceItem>;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceItem {
    pub id: <ProductSpec as super::BaseModel>::Id,
    pub timestamp: DateTime<Utc>,
    pub direction: Direction,
    pub cost: <ProductSpec as super::BaseModel>::Cost,
    pub count: <ProductSpec as super::BaseModel>::Count,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Direction {
    Pub,
    Sub,
}
