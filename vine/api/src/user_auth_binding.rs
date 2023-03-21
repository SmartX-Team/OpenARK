use ipis::core::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserAuthBinding",
    struct = "UserAuthBindingCrd",
    shortname = "uab",
    printcolumn = r#"{
        "name": "user",
        "type": "string",
        "description":"User name",
        "jsonPath":".spec.user"
    }"#,
    printcolumn = r#"{
        "name": "auth",
        "type": "string",
        "description":"UserAuth name",
        "jsonPath":".spec.auth"
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
pub struct UserAuthBindingSpec {
    pub user: String,
    pub auth: String,
    pub user_id: String,
    #[serde(default)]
    pub expired_timestamp: Option<DateTime<Utc>>,
}
