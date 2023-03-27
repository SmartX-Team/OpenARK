pub extern crate k8s_openapi;
pub extern crate kube;
pub extern crate serde_json;
pub extern crate serde_yaml;

pub mod function;
pub mod model;
pub mod model_storage_binding;
pub mod package;
pub mod pipe;
pub mod storage;

pub mod consts {
    pub use vine_api::consts::NAMESPACE;
}
