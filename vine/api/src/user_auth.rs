use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kiss.netai-cloud",
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
    Google {
        #[serde(flatten)]
        oauth2: UserAuthOAuth2Common,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserAuthOAuth2Common {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}
