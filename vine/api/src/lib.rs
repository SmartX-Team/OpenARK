pub extern crate email_address;
pub extern crate k8s_openapi;
pub extern crate kube;
pub extern crate serde_json;
pub extern crate serde_yaml;

pub mod user;
pub mod user_auth;
pub mod user_auth_binding;
pub mod user_box_binding;
pub mod user_box_quota;
pub mod user_box_quota_binding;
pub mod user_role;
pub mod user_role_binding;

pub mod consts {
    pub const NAMESPACE: &str = "vine";
}
