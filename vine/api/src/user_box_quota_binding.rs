use ipis::core::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kiss.netai-cloud",
    version = "v1alpha1",
    kind = "UserBoxQuotaBinding",
    struct = "UserBoxQuotaBindingCrd",
    shortname = "ubqb",
    printcolumn = r#"{
        "name": "user",
        "type": "string",
        "description":"User name",
        "jsonPath":".spec.user"
    }"#,
    printcolumn = r#"{
        "name": "quota",
        "type": "string",
        "description":"UserBoxQuota name",
        "jsonPath":".spec.quota"
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
pub struct UserBoxQuotaBindingSpec {
    pub user: String,
    pub quota: String,
    pub expired_timestamp: Option<DateTime<Utc>>,
}
