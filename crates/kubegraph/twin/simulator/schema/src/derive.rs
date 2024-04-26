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
            "auto" => self.is_auto(),
            "safe" => self.is_safe(),
            _ => match self.traits.get(key).copied() {
                Some(Some(value)) => value,
                Some(None) => true,
                None => false,
            },
        }
    }

    #[inline]
    pub fn is_auto(&self) -> bool {
        self.is_enabled_or_assume_true("auto")
    }

    #[inline]
    pub fn is_safe(&self) -> bool {
        self.is_enabled_or_assume_true("safe")
    }

    #[inline]
    pub fn is_twin(&self) -> bool {
        self.is_enabled("twin")
    }

    fn is_enabled_or_assume_true(&self, key: &str) -> bool {
        match self.traits.get(key).copied() {
            Some(Some(value)) => value,
            Some(None) => true,
            None => !self.is_twin(),
        }
    }
}
