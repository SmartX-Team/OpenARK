use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(
    Copy, Clone, Debug, Display, EnumString, PartialEq, Serialize, Deserialize, JsonSchema,
)]
pub enum ImagePullPolicy {
    Always,
    IfNotPresent,
    Never,
}
