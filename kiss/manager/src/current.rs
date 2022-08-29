use std::collections::BTreeMap;

use ipis::core::anyhow::{anyhow, Result};
use kiss_api::{
    k8s_openapi::{api::core::v1::ConfigMap, Resource},
    kube::{
        api::{Patch, PatchParams, PostParams},
        core::ObjectMeta,
        Api, Client,
    },
    serde_json::json,
};
use semver::Version;

pub struct Handler {
    api: Api<ConfigMap>,
}

impl Handler {
    pub async fn try_default() -> Result<Self> {
        // create a kubernetes client
        let ns = "kiss";
        let client = Client::try_default().await?;

        Ok(Self {
            api: Api::<ConfigMap>::namespaced(client, ns),
        })
    }
}

impl Handler {
    pub async fn create(&self, version: &Version) -> Result<()> {
        let config = ConfigMap {
            metadata: ObjectMeta {
                name: Some("manager".into()),
                ..Default::default()
            },
            immutable: Some(false),
            data: Some({
                let mut map = BTreeMap::default();
                map.insert("version".into(), version.to_string());
                map
            }),
            ..Default::default()
        };
        let pp = PostParams {
            field_manager: Some("kiss-manager".into()),
            ..Default::default()
        };
        self.api.create(&pp, &config).await?;
        Ok(())
    }

    pub async fn get(&self, latest: &Version) -> Result<Version> {
        let config = match self.api.get("manager").await {
            Ok(config) => config,
            Err(_) => {
                self.create(latest).await?;
                return Ok(latest.clone());
            }
        };

        let version = config
            .data
            .as_ref()
            .and_then(|map| map.get("version"))
            .ok_or_else(|| anyhow!("failed to find version field in configmap"))?;
        version.parse().map_err(Into::into)
    }

    pub async fn patch(&self, version: Version) -> Result<()> {
        let patch = Patch::Apply(json!({
            "apiVersion": ConfigMap::API_VERSION,
            "kind": ConfigMap::KIND,
            "spec": {
                "version": version.to_string(),
            },
        }));
        let pp = PatchParams::apply("kiss-manager").force();
        self.api.patch("manager", &pp, &patch).await?;
        Ok(())
    }
}
