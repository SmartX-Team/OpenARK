use serde::{Deserialize, Serialize};

use crate::function::webhook::NetworkFunctionWebhookSpec;

use super::product::ProductSpec;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubSpec {
    pub cost: <Self as super::BaseModel>::Cost,
    pub count: <Self as super::BaseModel>::Count,
    pub function: NetworkFunctionWebhookSpec,
}

impl super::BaseModel for SubSpec {
    type Id = <ProductSpec as super::BaseModel>::Id;
    type Cost = <ProductSpec as super::BaseModel>::Cost;
    type Count = <ProductSpec as super::BaseModel>::Count;
}
