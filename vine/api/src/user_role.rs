use std::{iter::Sum, ops};

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource,
)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserRole",
    struct = "UserRoleCrd",
    shortname = "ur",
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserRoleSpec {
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_dev: bool,
    #[serde(default)]
    pub is_ops: bool,
}

impl ops::BitOr for UserRoleSpec {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            is_admin: self.is_admin || rhs.is_admin,
            is_dev: self.is_dev || rhs.is_dev,
            is_ops: self.is_ops || rhs.is_ops,
        }
    }
}

impl Sum for UserRoleSpec {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(ops::BitOr::bitor).unwrap_or_default()
    }
}
