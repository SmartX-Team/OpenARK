use dash_api::storage::ModelStorageCrd;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct StorageContext {
    pub crd: ModelStorageCrd,
}
