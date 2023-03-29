use std::collections::BTreeMap;

use ipis::core::anyhow::Error;
use ort::session::{Input, Output};
use serde::{Deserialize, Serialize};

pub type TensorKindMap = BTreeMap<String, TensorKind>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "spec")]
pub enum TensorKind {
    Image(#[serde(default)] ImageKind),
    Text(#[serde(default)] TextKind),
}

impl TryFrom<&Input> for TensorKind {
    type Error = Error;

    fn try_from(value: &Input) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<&Output> for TensorKind {
    type Error = Error;

    fn try_from(value: &Output) -> Result<Self, Self::Error> {
        todo!()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageKind {}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextKind {}
