use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::user::UserSpec;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]

pub struct UserSessionRef {
    pub box_name: Option<String>,
    pub namespace: String,
    pub user: UserSpec,
    pub user_name: String,
}

#[cfg(feature = "client")]
#[derive(Clone)]
pub struct UserSessionMetadata<'a> {
    pub box_name: Option<String>,
    pub kube: &'a ::kube::Client,
    pub user: UserSpec,
    pub user_name: String,
}
