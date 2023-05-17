use std::{collections::BTreeMap, ops::Deref, str::FromStr};

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "User",
    struct = "UserCrd",
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
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserSpec {
    pub name: String,
    pub contact: UserContact,
    pub detail: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserStatus {
    pub last_box: Option<String>,
    pub last_updated: DateTime<Utc>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct UserContact {
    #[serde(default)]
    pub email: Option<EmailAddress>,
    #[serde(default)]
    pub tel_phone: Option<String>,
    #[serde(default)]
    pub tel_office: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EmailAddress(pub ::email_address::EmailAddress);

impl FromStr for EmailAddress {
    type Err = <::email_address::EmailAddress as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <::email_address::EmailAddress as FromStr>::from_str(s).map(Self)
    }
}

impl Deref for EmailAddress {
    type Target = ::email_address::EmailAddress;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialOrd for EmailAddress {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.as_str().partial_cmp(other.0.as_str())
    }
}

impl Ord for EmailAddress {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}

impl JsonSchema for EmailAddress {
    fn is_referenceable() -> bool {
        false
    }

    fn schema_name() -> String {
        "EmailAddress".into()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(gen)
    }
}
