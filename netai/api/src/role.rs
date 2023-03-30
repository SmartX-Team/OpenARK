use inflector::Inflector;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum Role {
    // NLP
    QuestionAnswering,
}

impl Role {
    pub fn to_string_kebab_case(&self) -> String {
        self.to_string().to_kebab_case()
    }
}
