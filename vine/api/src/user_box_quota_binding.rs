use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserBoxQuotaBinding",
    root = "UserBoxQuotaBindingCrd",
    shortname = "ubqb",
    printcolumn = r#"{
        "name": "user",
        "type": "string",
        "description": "User name",
        "jsonPath": ".spec.user"
    }"#,
    printcolumn = r#"{
        "name": "quota",
        "type": "string",
        "description": "UserBoxQuota name",
        "jsonPath": ".spec.quota"
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
pub struct UserBoxQuotaBindingSpec<Quota = String> {
    pub user: String,
    pub quota: Quota,
    #[serde(default)]
    pub expired_timestamp: Option<DateTime<Utc>>,
}
