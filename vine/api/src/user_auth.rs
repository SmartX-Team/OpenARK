use anyhow::{bail, Result};
use ark_core_k8s::data::Url;
use k8s_openapi::api::core::v1::NodeSpec;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::Display;
use uuid::Uuid;

use crate::{
    user::{EmailAddress, UserSpec},
    user_box_binding::UserBoxBindingSpec,
    user_box_quota::UserBoxQuotaSpec,
    user_box_quota_binding::UserBoxQuotaBindingSpec,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserAuth",
    struct = "UserAuthCrd",
    shortname = "ua",
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "model version",
        "jsonPath": ".metadata.generation"
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserAuthLoginQuery {
    pub box_uuid: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserAuthOAuth2Common {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UserAuthPayload {
    /// User e-mail address
    email: String,
    /// User name
    name: String,
    /// Preferred user name
    preferred_username: String,
}

impl UserAuthPayload {
    pub fn primary_key(&self) -> Result<String> {
        fn encode(s: &str) -> String {
            s.to_lowercase()
                // common special words
                .replace('-', "-s-")
                .replace('@', "-at-")
                // other special words
                .replace('_', "-u-")
        }

        match self.email.parse::<EmailAddress>() {
            Ok(email) => Ok(format!("email-{}", encode(email.0.as_str()))),
            Err(_) => match self.preferred_username.as_str() {
                "" => bail!("failed to parse primary key: {:?}", self),
                name => Ok(format!("name-{}", encode(name))),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "status", content = "data")]
pub enum UserAuthResponse {
    Accept {
        box_bindings: Vec<UserBoxBindingSpec<NodeSpec>>,
        box_name: Option<String>,
        box_quota_bindings: Vec<UserBoxQuotaBindingSpec<UserBoxQuotaSpec>>,
        user: UserSpec,
    },
    Error(UserAuthError),
}

impl From<UserAuthError> for UserAuthResponse {
    fn from(error: UserAuthError) -> Self {
        Self::Error(error)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "status", content = "data")]
pub enum UserSessionResponse {
    Accept {
        box_quota: Option<UserBoxQuotaSpec>,
        user: UserSpec,
    },
    Error(UserSessionError),
}

impl From<UserAuthError> for UserSessionResponse {
    fn from(error: UserAuthError) -> Self {
        Self::Error(error.into())
    }
}

#[derive(Clone, Debug, Display, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "status", content = "data")]
pub enum UserSessionError {
    AlreadyLoggedInByNode { node_name: String },
    AlreadyLoggedInByUser { user_name: String },
    AuthError(UserAuthError),
    Deny { user: UserSpec },
    NodeNotFound,
    NodeNotInCluster,
}

impl From<UserAuthError> for UserSessionError {
    fn from(error: UserAuthError) -> Self {
        Self::AuthError(error)
    }
}

#[derive(Clone, Debug, Display, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "status", content = "data")]
pub enum UserAuthError {
    AuthorizationTokenMalformed,
    AuthorizationTokenNotFound,
    NamespaceNotAllowed,
    NamespaceTokenMalformed,
    PrimaryKeyMalformed,
    UserNotRegistered,
}
