use anyhow::{anyhow, bail, Result};
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};
use sha2::{Digest, Sha256};
use tracing::{instrument, Level};

/// TODO: more generic and stable one?
#[instrument(level = Level::INFO, skip_all, err(Display))]
pub async fn get_cluster_name(kube: Client) -> Result<String> {
    let api = Api::<ConfigMap>::namespaced(kube, "kube-public");
    let configmap = api
        .get("cluster-info")
        .await
        .map_err(|error| anyhow!("failed to get kube config: {error}"))?;

    match configmap
        .data
        .as_ref()
        .and_then(|data| data.get("kubeconfig"))
    {
        Some(config) => {
            // read hash digest and consume hasher
            let hash = Sha256::digest(config.as_bytes());

            // encode to hex format
            Ok(format!("{hash:x}"))
        }
        None => bail!("no kube config"),
    }
}
