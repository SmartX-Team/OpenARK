use std::{collections::BTreeMap, str::FromStr};

use dash_api::model::ModelFieldSpec;
use ipis::core::anyhow::{anyhow, Error, Result};
use kiss_api::serde_json::Value;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputTemplate {
    map: BTreeMap<String, ModelFieldSpec>,
}

impl InputTemplate {
    pub fn update_fields(&mut self, fields: Vec<SetField>) -> Result<()> {
        fields
            .into_iter()
            .try_for_each(|field| self.update_field(field))
    }

    pub fn update_field(&mut self, field: SetField) -> Result<()> {
        todo!()
    }

    pub fn to_json(&self) -> Value {
        todo!()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetField {
    pub name: String,
    pub value: String,
}

impl FromStr for SetField {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let re = Regex::new(crate::name::RE_SET)?;
        re.captures(s)
            .and_then(|captures| captures.iter().flatten().last())
            .map(|m| Self {
                name: s[..m.start()].to_string(),
                value: s[m.start()..m.end() - 1].to_string(),
            })
            .ok_or_else(|| anyhow!("field name is invalid: {s} {s:?}"))
    }
}
