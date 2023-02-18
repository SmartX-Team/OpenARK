use ipis::core::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kiss.netai-cloud",
    version = "v1alpha1",
    kind = "UserBoxBinding",
    struct = "UserBoxBindingCrd",
    shortname = "ubb",
    printcolumn = r#"{
        "name": "user",
        "type": "string",
        "description":"User name",
        "jsonPath":".spec.user"
    }"#,
    printcolumn = r#"{
        "name": "box",
        "type": "string",
        "description":"Box name",
        "jsonPath":".spec.box"
    }"#,
    printcolumn = r#"{
        "name": "autologin",
        "type": "boolean",
        "description":"Whether the box is automatically logged-in",
        "jsonPath":".spec.autologin"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "expired-at",
        "type": "date",
        "description":"expired time",
        "jsonPath":".spec.expiredTimestamp"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxBindingSpec {
    pub user: String,
    pub r#box: String,
    pub autologin: bool,
    pub expired_timestamp: Option<DateTime<Utc>>,
}
