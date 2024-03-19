use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NetworkDerive {
    #[serde(default, flatten)]
    pub traits: BTreeMap<String, Option<bool>>,
}

impl NetworkDerive {
    pub fn is_enabled(&self, key: &str) -> bool {
        match key {
            "safe" => self.is_safe(),
            _ => match self.traits.get("safe").copied() {
                Some(Some(value)) => value,
                Some(None) => true,
                None => false,
            },
        }
    }

    pub fn is_safe(&self) -> bool {
        match self.traits.get("safe").copied() {
            Some(Some(value)) => value,
            Some(None) | None => true,
        }
    }
}
