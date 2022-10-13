use ipis::core::anyhow::Result;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};

use crate::config::infer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnsibleConfig {
    pub image: String,
}

impl AnsibleConfig {
    pub async fn try_default(kube: &Client) -> Result<Self> {
        let ns = crate::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
        let config = api.get("ansible-config").await?;

        Ok(Self {
            image: infer(&config, "kubespray_image")?,
        })
    }
}
