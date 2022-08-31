use std::collections::BTreeMap;

use ipis::{
    core::anyhow::{anyhow, bail, Result},
    log::{info, warn},
};
use kiss_api::{
    k8s_openapi::{
        api::{
            batch::v1::{Job, JobSpec},
            core::v1::{ConfigMap, Container, EnvVar, PodSpec, PodTemplateSpec},
        },
        Resource,
    },
    kube::{
        api::{DeleteParams, ListParams, Patch, PatchParams, PostParams},
        core::ObjectMeta,
        Api, Client, ResourceExt,
    },
    serde_json::json,
};
use semver::Version;

pub struct Handler {
    api_config: Api<ConfigMap>,
    api_job: Api<Job>,
}

impl Handler {
    const NAMESPACE: &'static str = "kiss";

    pub async fn try_default() -> Result<Self> {
        // create a kubernetes client
        let client = Client::try_default().await?;

        Ok(Self {
            api_config: Api::namespaced(client.clone(), Self::NAMESPACE),
            api_job: Api::namespaced(client, Self::NAMESPACE),
        })
    }
}

impl Handler {
    async fn create_config(&self, version: &Version) -> Result<()> {
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
        self.api_config.create(&pp, &config).await?;
        Ok(())
    }

    pub async fn get_version(&self, latest: &Version) -> Result<Option<Version>> {
        if !self.update_job_status().await? {
            return Ok(None);
        }

        let config = match self.api_config.get_opt("manager").await? {
            Some(config) => config,
            None => {
                self.create_config(latest).await?;
                return Ok(Some(latest.clone()));
            }
        };

        let version = config
            .data
            .as_ref()
            .and_then(|map| map.get("version"))
            .ok_or_else(|| anyhow!("failed to find version field in configmap"))?;
        version.parse().map(Some).map_err(Into::into)
    }

    async fn patch_version(&self, version: &Version) -> Result<()> {
        let patch = Patch::Apply(json!({
            "apiVersion": ConfigMap::API_VERSION,
            "kind": ConfigMap::KIND,
            "data": {
                "version": version.to_string(),
            },
        }));
        let pp = PatchParams::apply("kiss-manager").force();
        self.api_config.patch("manager", &pp, &patch).await?;
        Ok(())
    }
}

impl Handler {
    const UPGRADE_SERVICE_TYPE: &'static str = "netai-cloud-upgrade-kiss";

    async fn update_job_status(&self) -> Result<bool> {
        // load the previous jobs
        let lp = ListParams {
            label_selector: Some(format!("serviceType={}", Self::UPGRADE_SERVICE_TYPE)),
            ..Default::default()
        };
        let jobs = self.api_job.list(&lp).await?.items;

        match &jobs[..] {
            // no jobs are running
            [] => Ok(true),
            // a job has run
            [job] => {
                let name = job.name_any();
                let status = job.status.as_ref();

                let has_completed = status.and_then(|e| e.succeeded).unwrap_or_default() > 0;
                let has_failed = status.and_then(|e| e.failed).unwrap_or_default() > 0;

                // remove the job if finished
                if has_completed || has_failed {
                    let dp = DeleteParams::background();
                    self.api_job.delete(&name, &dp).await?;
                }

                // when the job is succeeded
                if has_completed {
                    info!("Job is completed: {name}");

                    // parse version tag
                    let version = job
                        .labels()
                        .get("targetVersion")
                        .ok_or_else(|| anyhow!("failed to parse target version from Job"))
                        .and_then(|e| e.parse().map_err(Into::into))?;

                    // update version tag
                    self.patch_version(&version).await?;
                    Ok(true)
                }
                // when the job is failed
                else if has_failed {
                    warn!("Failed upgrading cluster: {name:?}");

                    // no changes are applied
                    Ok(true)
                }
                // when the job is not finished yet
                else {
                    info!("Job is running: {name}");
                    Ok(false)
                }
            }
            _ => bail!(
                "Detected upgrade job conflict: {:?}",
                jobs.iter().map(|job| job.name_any()).collect::<Vec<_>>(),
            ),
        }
    }

    pub async fn upgrade(&self, current: &Version, latest: &Version) -> Result<()> {
        // spawn a upgrade job
        let metadata = ObjectMeta {
            name: Some(format!("kiss-upgrade-v{}", latest)),
            namespace: Some(Self::NAMESPACE.into()),
            labels: Some(
                vec![
                    ("serviceType".into(), Self::UPGRADE_SERVICE_TYPE.into()),
                    ("sourceVersion".into(), current.to_string()),
                    ("targetVersion".into(), latest.to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let job = Job {
            spec: Some(JobSpec {
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: metadata.labels.clone(),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        restart_policy: Some("OnFailure".into()),
                        service_account: Some("kiss-controller".into()),
                        containers: vec![Container {
                            name: "kubectl".into(),
                            image: Some(
                                "ghcr.io/ulagbulag-village/netai-cloud-upgrade-kiss:master".into(),
                            ),
                            image_pull_policy: Some("Always".into()),
                            env: Some(vec![
                                EnvVar {
                                    name: "VERSION_SRC".into(),
                                    value: Some(current.to_string()),
                                    ..Default::default()
                                },
                                EnvVar {
                                    name: "VERSION_TGT".into(),
                                    value: Some(latest.to_string()),
                                    ..Default::default()
                                },
                            ]),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            metadata,
            ..Default::default()
        };
        let pp = PostParams {
            dry_run: false,
            field_manager: Some("kube-manager".into()),
        };
        info!("Spawning a job: {}", job.name_any());
        self.api_job.create(&pp, &job).await?;
        Ok(())
    }
}
