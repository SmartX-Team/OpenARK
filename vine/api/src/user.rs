use ipis::core::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kiss.netai-cloud",
    version = "v1alpha1",
    kind = "User",
    struct = "UserCrd",
    status = "UserStatus",
    shortname = "u",
    printcolumn = r#"{
        "name": "email",
        "type": "string",
        "description":"email address",
        "jsonPath":".spec.contact.email"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "updated-at",
        "type": "date",
        "description":"updated time",
        "jsonPath":".status.lastUpdated"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserSpec {
    pub name: String,
    pub contact: UserContact,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserStatus {
    pub last_updated: DateTime<Utc>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct UserContact {
    pub email: Option<EmailAddress>,
    pub tel_phone: Option<String>,
    pub tel_office: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EmailAddress(pub ::email_address::EmailAddress);

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
