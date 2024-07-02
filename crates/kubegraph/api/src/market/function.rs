use serde::{Deserialize, Serialize};

use super::transaction::TransactionTemplate;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketFunctionContext {
    pub template: TransactionTemplate,
}
