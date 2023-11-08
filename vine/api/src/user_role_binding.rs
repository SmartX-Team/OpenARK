use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserRoleBinding",
    root = "UserRoleBindingCrd",
    shortname = "urb",
    printcolumn = r#"{
        "name": "user",
        "type": "string",
        "description": "User name",
        "jsonPath": ".spec.user"
    }"#,
    printcolumn = r#"{
        "name": "role",
        "type": "string",
        "description": "UserRole name",
        "jsonPath": ".spec.role"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "expired-at",
        "type": "date",
        "description": "expired time",
        "jsonPath": ".spec.expiredTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "model version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserRoleBindingSpec<Role = String> {
    pub user: String,
    pub role: Role,
    #[serde(default)]
    pub expired_timestamp: Option<DateTime<Utc>>,
}
