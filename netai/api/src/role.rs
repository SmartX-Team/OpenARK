use ipis::core::anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::tensor::TensorKindMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {}

impl Role {
    pub fn try_from_io(inputs: &TensorKindMap, outputs: &TensorKindMap) -> Result<Self> {
        todo!()
    }
}
