use serde::{Deserialize, Serialize};

use super::trade::TradeTemplate;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketFunctionContext {
    pub template: TradeTemplate,
}
