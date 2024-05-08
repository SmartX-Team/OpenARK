use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct FunctionMetadata {
    pub name: String,
}

impl FunctionMetadata {
    pub const NAME_STATIC: &'static str = "static";
}
