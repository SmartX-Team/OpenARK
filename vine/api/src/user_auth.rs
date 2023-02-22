use ipis::core::{anyhow::Result, uuid::Uuid};
use kiss_api::r#box::BoxSpec;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    user::{EmailAddress, UserSpec},
    user_box_binding::UserBoxBindingSpec,
    user_box_quota::UserBoxQuotaSpec,
    user_box_quota_binding::UserBoxQuotaBindingSpec,
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.netai-cloud",
    version = "v1alpha1",
    kind = "UserAuth",
    struct = "UserAuthCrd",
    shortname = "ua",
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub enum UserAuthSpec {
    OIDC {
        #[serde(flatten)]
        oauth2: UserAuthOAuth2Common,
        issuer: Url,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserAuthLoginQuery {
    pub box_uuid: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserAuthOAuth2Common {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UserAuthPayload {
    /// User e-mail address
    email: EmailAddress,
    /// User name
    name: String,
}

impl UserAuthPayload {
    pub fn primary_key(&self) -> Result<String> {
        // TODO: verify email address
        Ok(format!(
            "email-{}",
            self.email
                .0
                .as_str()
                .to_lowercase()
                // common special words
                .replace('-', "-s-")
                .replace('@', "-at-")
                // other special words
                .replace('_', "-u-")
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "status", content = "data")]
pub enum UserAuthResponse {
    Accept {
        box_bindings: Vec<UserBoxBindingSpec<BoxSpec>>,
        box_quota_bindings: Vec<UserBoxQuotaBindingSpec<UserBoxQuotaSpec>>,
        user: UserSpec,
    },
    AuthorizationTokenMalformed,
    AuthorizationTokenNotFound,
    PrimaryKeyMalformed,
    UserNotRegistered,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Url(pub ::url::Url);

impl PartialOrd for Url {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.as_str().partial_cmp(other.0.as_str())
    }
}

impl Ord for Url {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}

impl JsonSchema for Url {
    fn is_referenceable() -> bool {
        false
    }

    fn schema_name() -> String {
        "Url".into()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(gen)
    }
}
