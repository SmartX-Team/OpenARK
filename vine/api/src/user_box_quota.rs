use k8s_openapi::api::core::v1::ResourceRequirements;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserBoxQuota",
    struct = "UserBoxQuotaCrd",
    shortname = "ubq",
    printcolumn = r#"{
        "name": "amount",
        "type": "number",
        "description":"the number of allowed boxes",
        "jsonPath":".spec.amount"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaSpec {
    pub resources: ResourceRequirements,
}
