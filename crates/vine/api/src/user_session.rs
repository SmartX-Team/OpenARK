use std::sync::Arc;

use k8s_openapi::api::core::v1::NodeSpec;
use kube::Client;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    user::UserCrd, user_box_binding::UserBoxBindingSpec, user_box_quota::UserBoxQuotaSpec,
    user_box_quota_binding::UserBoxQuotaBindingSpec, user_role::UserRoleSpec,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSession {
    pub box_bindings: Arc<Vec<UserBoxBindingSpec<NodeSpec>>>,
    pub box_name: Option<String>,
    pub box_quota_bindings: Arc<Vec<UserBoxQuotaBindingSpec<UserBoxQuotaSpec>>>,
    #[serde(skip)]
    pub kube: Option<Client>,
    pub namespace: String,
    pub role: UserRoleSpec,
    #[serde(skip)]
    pub token: Option<String>,
    pub user: Arc<UserCrd>,
    pub user_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserSessionCommandBatch<Command = UserSessionCommand, UserNames = Vec<String>> {
    pub command: Command,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub user_names: Option<UserNames>,
    #[serde(default)]
    pub wait: bool,
}

pub type UserSessionCommand = Vec<String>;
