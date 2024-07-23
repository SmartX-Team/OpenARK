use std::collections::BTreeMap;

use ark_core_k8s::data::EmailAddress;
use chrono::{DateTime, Utc};
use kube::{CustomResource, ResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "User",
    root = "UserCrd",
    status = "UserStatus",
    shortname = "u",
    printcolumn = r#"{
        "name": "real name",
        "type": "string",
        "description": "user's real name",
        "jsonPath": ".spec.name"
    }"#,
    printcolumn = r#"{
        "name": "email",
        "type": "string",
        "description": "email address",
        "jsonPath": ".spec.contact.email"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "updated-at",
        "type": "date",
        "description": "updated time",
        "jsonPath": ".status.lastUpdated"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "user version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserSpec {
    #[serde(default)]
    pub alias: Option<String>,
    pub name: String,
    #[serde(default)]
    pub contact: UserContact,
    #[serde(default)]
    pub detail: BTreeMap<String, String>,
}

impl UserCrd {
    pub fn perferred_name(&self) -> String {
        self.spec.alias.clone().unwrap_or_else(|| self.name_any())
    }

    pub fn user_namespace(&self) -> String {
        Self::user_namespace_with(&self.perferred_name())
    }

    pub fn user_namespace_with(user_name: &str) -> String {
        format!("vine-session-{user_name}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserStatus {
    pub last_box: Option<String>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserContact {
    #[serde(default)]
    pub email: Option<EmailAddress>,
    #[serde(default)]
    pub tel_phone: Option<String>,
    #[serde(default)]
    pub tel_office: Option<String>,
}
