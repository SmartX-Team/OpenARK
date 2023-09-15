use std::{borrow::Cow, collections::BTreeMap, fmt, io::Write};

use anyhow::{anyhow, bail, Error, Result};
use ark_core_k8s::domain::get_cluster_domain;
use byte_unit::Byte;
use chrono::Utc;
use dash_api::{
    model::{ModelCrd, ModelCustomResourceDefinitionRefSpec},
    model_storage_binding::{
        ModelStorageBindingStorageSourceSpec, ModelStorageBindingStorageSpec,
        ModelStorageBindingSyncPolicy, ModelStorageBindingSyncPolicyPull,
        ModelStorageBindingSyncPolicyPush,
    },
    storage::object::{
        ModelStorageObjectBorrowedSpec, ModelStorageObjectClonedSpec,
        ModelStorageObjectOwnedReplicationSpec, ModelStorageObjectOwnedSpec,
        ModelStorageObjectRefSecretRefSpec, ModelStorageObjectRefSpec, ModelStorageObjectSpec,
    },
};
use futures::{future::try_join_all, TryFutureExt};
use k8s_openapi::{
    api::{
        batch::v1::{Job, JobSpec},
        core::v1::{
            Affinity, Container, EnvVar, NodeAffinity, NodeSelector, NodeSelectorRequirement,
            NodeSelectorTerm, PodSpec, PodTemplateSpec, PreferredSchedulingTerm,
            ResourceRequirements, Secret, Service,
        },
        networking::v1::{
            HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
            IngressServiceBackend, IngressSpec, ServiceBackendPort,
        },
    },
    apimachinery::pkg::api::resource::Quantity,
};
use kube::{
    api::PostParams,
    core::{DynamicObject, ObjectMeta, TypeMeta},
    Api, Client, ResourceExt,
};
use minio::s3::{
    args::{
        BucketExistsArgs, GetBucketReplicationArgs, GetObjectArgs, ListObjectsV2Args,
        MakeBucketArgs, SetBucketReplicationArgs, SetBucketVersioningArgs,
    },
    creds::{Credentials, Provider, StaticProvider},
    http::BaseUrl,
    types::{Destination, ReplicationConfig, ReplicationRule},
    utils::Multimap,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub struct ObjectStorageClient {
    source: Option<(ObjectStorageRef, ModelStorageBindingSyncPolicy)>,
    source_binding_name: Option<String>,
    target: ObjectStorageRef,
}

struct ObjectStorageRef {
    base_url: BaseUrl,
    endpoint: Url,
    name: String,
    provider: StaticProvider,
}

impl ObjectStorageClient {
    pub async fn try_new<'source>(
        kube: &Client,
        namespace: &str,
        storage: ModelStorageBindingStorageSpec<'source, &ModelStorageObjectSpec>,
    ) -> Result<Self> {
        Ok(Self {
            source: match storage.source {
                Some(ModelStorageBindingStorageSourceSpec {
                    name: source_name,
                    storage: source,
                    sync_policy,
                }) => Some(
                    ObjectStorageRef::load_storage_provider(kube, namespace, source_name, source)
                        .await
                        .map(|source| (source, sync_policy))?,
                ),
                None => None,
            },
            source_binding_name: storage.source_binding_name.map(Into::into),
            target: ObjectStorageRef::load_storage_provider(
                kube,
                namespace,
                storage.target_name,
                storage.target,
            )
            .await?,
        })
    }

    pub fn get_session<'model>(
        &self,
        kube: &'model Client,
        namespace: &'model str,
        model: &'model ModelCrd,
    ) -> ObjectStorageSession<'_, 'model, '_> {
        ObjectStorageSession {
            kube,
            model,
            namespace,
            source: self
                .source
                .as_ref()
                .map(|(source, sync_policy)| (source, *sync_policy)),
            source_binding_name: self.source_binding_name.as_deref(),
            target: self.target.get_client(),
            target_ref: &self.target,
        }
    }
}

impl<'model> ObjectStorageRef {
    async fn load_storage_provider(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectSpec,
    ) -> Result<Self> {
        match storage {
            ModelStorageObjectSpec::Borrowed(storage) => {
                Self::load_storage_provider_by_borrowed(kube, namespace, name, storage).await
            }
            ModelStorageObjectSpec::Cloned(storage) => {
                Self::load_storage_provider_by_cloned(kube, namespace, name, storage).await
            }
            ModelStorageObjectSpec::Owned(storage) => {
                Self::load_storage_provider_by_owned(kube, namespace, name, storage).await
            }
        }
        .map_err(|error| anyhow!("failed to load object storage provider: {error}"))
    }

    async fn load_storage_provider_by_borrowed(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectBorrowedSpec,
    ) -> Result<Self> {
        let ModelStorageObjectBorrowedSpec { reference } = storage;
        Self::load_storage_provider_by_reference(kube, namespace, name, reference).await
    }

    async fn load_storage_provider_by_cloned(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectClonedSpec,
    ) -> Result<Self> {
        let reference =
            Self::load_storage_provider_by_reference(kube, namespace, name, &storage.reference)
                .await?;
        let owned =
            Self::load_storage_provider_by_owned(kube, namespace, name, &storage.owned).await?;

        let admin = MinioAdminClient {
            storage: &reference,
        };
        // TODO: verify and join endpoint
        if !admin.is_site_replication_enabled().await? {
            admin.add_site_replication(&owned).await?;
        }
        Ok(owned)
    }

    async fn load_storage_provider_by_owned(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectOwnedSpec,
    ) -> Result<Self> {
        let storage = Self::create_or_get_storage(kube, namespace, storage).await?;
        Self::load_storage_provider_by_reference(kube, namespace, name, &storage).await
    }

    async fn load_storage_provider_by_reference(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectRefSpec,
    ) -> Result<Self> {
        let ModelStorageObjectRefSpec {
            endpoint,
            secret_ref:
                ModelStorageObjectRefSecretRefSpec {
                    map_access_key,
                    map_secret_key,
                    name: secret_name,
                },
        } = storage;

        let mut secret = match {
            let api = Api::<Secret>::namespaced(kube.clone(), namespace);
            api.get_opt(secret_name).await?
        } {
            Some(secret) => secret,
            None => bail!("no such secret: {secret_name}"),
        };

        let mut get_secret_data =
            |key: &str| match secret.data.as_mut().and_then(|data| data.remove(key)) {
                Some(value) => String::from_utf8(value.0).map_err(|error| {
                    anyhow!("failed to parse secret key ({secret_name}/{key}): {error}")
                }),
                None => bail!("no such secret key: {secret_name}/{key}"),
            };
        let access_key = get_secret_data(map_access_key)?;
        let secret_key = get_secret_data(map_secret_key)?;

        Ok(Self {
            base_url: BaseUrl::from_string(endpoint.to_string())?,
            endpoint: endpoint.0.clone(),
            name: name.to_string(),
            provider: StaticProvider::new(&access_key, &secret_key, None),
        })
    }

    async fn create_or_get_storage(
        kube: &Client,
        namespace: &str,
        storage: &ModelStorageObjectOwnedSpec,
    ) -> Result<ModelStorageObjectRefSpec> {
        async fn get_latest_minio_image() -> Result<String> {
            Ok("docker.io/minio/minio:latest".into())
        }

        let tenant_name = "object-storage";
        let labels = {
            let mut map: BTreeMap<String, String> = BTreeMap::default();
            map.insert("v1.min.io/tenant".into(), tenant_name.to_string());
            map
        };

        let secret_user_0 = {
            let name = tenant_name;
            let pool_name = "pool-0";

            let spec = MinioTenantSpec {
                image: get_latest_minio_image().await?,
                labels: &labels,
                pool_name,
                storage,
            };
            get_or_create_minio_tenant(kube, namespace, name, spec).await?
        };

        {
            let name = tenant_name;
            let api_ingress = Api::<Ingress>::namespaced(kube.clone(), namespace);
            get_or_create_ingress(
                &api_ingress,
                namespace,
                name,
                Some(&labels),
                IngressServiceBackend {
                    name: "minio".into(),
                    port: Some(ServiceBackendPort {
                        name: Some("http-minio".into()),
                        ..Default::default()
                    }),
                },
            )
            .await?
        };

        let minio_domain = {
            let api = Api::<Service>::namespaced(kube.clone(), namespace);
            match api.get_opt("minio").await?.and_then(|service| {
                service
                    .status
                    .and_then(|status| status.load_balancer)
                    .and_then(|load_balancer| load_balancer.ingress)
                    .and_then(|ingresses| {
                        ingresses
                            .into_iter()
                            .filter_map(|ingress| ingress.ip)
                            .next()
                    })
            }) {
                Some(ip) => ip,
                None => get_kubernetes_minio_domain(namespace).await?,
            }
        };

        Ok(ModelStorageObjectRefSpec {
            endpoint: format!("http://{minio_domain}/").parse()?,
            secret_ref: ModelStorageObjectRefSecretRefSpec {
                map_access_key: "CONSOLE_ACCESS_KEY".into(),
                map_secret_key: "CONSOLE_SECRET_KEY".into(),
                name: secret_user_0.name_any(),
            },
        })
    }
}

pub struct ObjectStorageSession<'client, 'model, 'source> {
    kube: &'model Client,
    model: &'model ModelCrd,
    namespace: &'model str,
    source: Option<(&'source ObjectStorageRef, ModelStorageBindingSyncPolicy)>,
    source_binding_name: Option<&'client str>,
    target: ::minio::s3::client::Client<'client>,
    target_ref: &'source ObjectStorageRef,
}

impl<'client, 'model, 'source> ObjectStorageSession<'client, 'model, 'source> {
    fn get_bucket_name(&self) -> String {
        self.model.name_any()
    }

    fn admin(&self) -> MinioAdminClient<'_> {
        MinioAdminClient {
            storage: self.target_ref,
        }
    }

    fn inverted(&self, bucket: &'client str) -> Result<(Self, &'client str)>
    where
        'source: 'client,
    {
        match self.source.as_ref() {
            Some((source_ref, sync_policy)) => Ok((
                Self {
                    kube: self.kube,
                    model: self.model,
                    namespace: self.namespace,
                    source: Some((self.target_ref, *sync_policy)),
                    source_binding_name: Some(bucket),
                    target: source_ref.get_client(),
                    target_ref: source_ref,
                },
                self.source_binding_name.unwrap_or(bucket),
            )),
            None => bail!("cannot invert object storage session without source storage"),
        }
    }

    async fn is_bucket_exists(&self) -> Result<bool> {
        let bucket_name = self.get_bucket_name();
        self.target
            .bucket_exists(&BucketExistsArgs::new(&bucket_name)?)
            .await
            .map_err(|error| anyhow!("failed to check bucket ({bucket_name}): {error}"))
    }

    pub async fn get(&self, ref_name: &str) -> Result<Option<Value>> {
        let bucket_name = self.get_bucket_name();
        let args = GetObjectArgs::new(&bucket_name, ref_name)?;

        match self.target.get_object(&args).await {
            Ok(response) => response.json().await.map_err(|error| {
                anyhow!("failed to parse object ({bucket_name}/{ref_name}): {error}")
            }),
            Err(error) => match &error {
                ::minio::s3::error::Error::S3Error(response) if response.code == "NoSuchKey" => {
                    Ok(None)
                }
                _ => bail!("failed to get object ({bucket_name}/{ref_name}): {error}"),
            },
        }
    }

    pub async fn get_list(&self) -> Result<Vec<Value>> {
        const LIMIT: u16 = 30;

        let bucket_name = self.get_bucket_name();
        let mut args = ListObjectsV2Args::new(&bucket_name)?;
        args.max_keys = Some(LIMIT);

        match self.target.list_objects_v2(&args).await {
            Ok(response) => try_join_all(
                response
                    .contents
                    .into_iter()
                    .map(|item| async move { self.get(&item.name).await }),
            )
            .await
            .map(|values| values.into_iter().flatten().collect())
            .map_err(|error| anyhow!("failed to list object ({bucket_name}): {error}")),
            Err(error) => bail!("failed to list object ({bucket_name}): {error}"),
        }
    }

    pub async fn create_bucket(&self) -> Result<()> {
        let mut bucket_name = self.get_bucket_name();
        if !self.is_bucket_exists().await? {
            let args = MakeBucketArgs::new(&bucket_name)?;
            bucket_name = match self.target.make_bucket(&args).await {
                Ok(response) => response.bucket_name,
                Err(error) => bail!("failed to create a bucket ({bucket_name}): {error}"),
            };
        }

        self.sync_bucket(bucket_name).await
    }

    async fn sync_bucket(&self, bucket_name: String) -> Result<()> {
        match &self.source {
            Some((source, ModelStorageBindingSyncPolicy { pull, push })) => {
                match pull {
                    ModelStorageBindingSyncPolicyPull::Always => {
                        self.sync_bucket_pull_always(&bucket_name).await?
                    }
                    ModelStorageBindingSyncPolicyPull::OnCreate => {
                        self.sync_bucket_pull_on_create(source, &bucket_name)
                            .await?
                    }
                    ModelStorageBindingSyncPolicyPull::Never => (),
                }
                match push {
                    ModelStorageBindingSyncPolicyPush::Always => {
                        self.sync_bucket_push_always(source, &bucket_name).await?
                    }
                    ModelStorageBindingSyncPolicyPush::OnDelete => (),
                    ModelStorageBindingSyncPolicyPush::Never => (),
                }
                Ok(())
            }
            None => Ok(()),
        }
        .map_err(|error: Error| anyhow!("failed to sync a bucket ({bucket_name}): {error}"))
    }

    async fn sync_bucket_pull_always(&self, bucket: &str) -> Result<()> {
        self.target
            .set_bucket_versioning(&SetBucketVersioningArgs::new(bucket, true)?)
            .await
            .map_err(|error| {
                anyhow!("failed to enable bucket versioning for Pulling ({bucket}): {error}")
            })?;

        let (source_session, bucket) = self.inverted(bucket)?;
        let source = self.target_ref;
        source_session.sync_bucket_push_always(source, bucket).await
    }

    async fn sync_bucket_pull_on_create(
        &self,
        source: &ObjectStorageRef,
        bucket: &str,
    ) -> Result<()> {
        let spec = BucketJobSpec {
            source: Some(source),
            sync_source: true,
            ..Default::default()
        };
        self.get_or_create_bucket_job(bucket, "pull", spec).await
    }

    async fn sync_bucket_push_always(&self, source: &ObjectStorageRef, bucket: &str) -> Result<()> {
        self.target
            .set_bucket_versioning(&SetBucketVersioningArgs::new(bucket, true)?)
            .await
            .map_err(|error| {
                anyhow!("failed to enable bucket versioning for Pushing ({bucket}): {error}")
            })?;

        let bucket_arn = self
            .admin()
            .set_remote_target(source, self.source_binding_name, bucket)
            .await?;

        let mut rules = self
            .target
            .get_bucket_replication(&GetBucketReplicationArgs {
                bucket,
                ..Default::default()
            })
            .await
            .map(|response| response.config.rules)
            .unwrap_or_default();

        if rules
            .iter()
            .any(|rule| rule.destination.bucket_arn == bucket_arn)
        {
            return Ok(());
        } else {
            rules.push(ReplicationRule {
                destination: Destination {
                    bucket_arn: bucket_arn.clone(),
                    access_control_translation: None,
                    account: None,
                    encryption_config: None,
                    metrics: None,
                    replication_time: None,
                    storage_class: None,
                },
                delete_marker_replication_status: Some(true),
                existing_object_replication_status: Some(true),
                filter: None,
                id: Some(bucket_arn.clone()),
                prefix: None,
                priority: rules
                    .iter()
                    .map(|rule| rule.priority.unwrap_or(1))
                    .max()
                    .unwrap_or_default()
                    .checked_add(1),
                source_selection_criteria: None,
                delete_replication_status: Some(true),
                status: true,
            });
        }

        self.target
            .set_bucket_replication(&SetBucketReplicationArgs {
                extra_headers: None,
                extra_query_params: None,
                region: None,
                bucket,
                config: &ReplicationConfig {
                    role: if rules.len() == 1 {
                        Some(bucket_arn)
                    } else {
                        None
                    },
                    rules,
                },
            })
            .await?;
        Ok(())
    }

    pub async fn delete_bucket(&self) -> Result<()> {
        let bucket_name = self.get_bucket_name();
        if self.is_bucket_exists().await? {
            if self.unsync_bucket(Some(bucket_name.clone()), true).await? {
                let spec = BucketJobSpec {
                    delete_target: true,
                    ..Default::default()
                };
                self.get_or_create_bucket_job(&bucket_name, "delete", spec)
                    .await
                    .map_err(|error| anyhow!("failed to delete a bucket ({bucket_name}): {error}"))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    pub async fn unsync_bucket(
        &self,
        bucket_name: Option<String>,
        delete_bucket: bool,
    ) -> Result<bool> {
        let bucket_name = bucket_name.unwrap_or_else(|| self.get_bucket_name());
        match &self.source {
            Some((source, ModelStorageBindingSyncPolicy { pull, push })) => {
                let delete = match pull {
                    ModelStorageBindingSyncPolicyPull::Always => {
                        self.unsync_bucket_pull_always(source, &bucket_name).await?
                    }
                    ModelStorageBindingSyncPolicyPull::OnCreate => true,
                    ModelStorageBindingSyncPolicyPull::Never => true,
                };
                let delete = delete
                    & match push {
                        ModelStorageBindingSyncPolicyPush::Always => {
                            self.unsync_bucket_push_always(source, &bucket_name).await?
                        }
                        ModelStorageBindingSyncPolicyPush::OnDelete => {
                            self.unsync_bucket_push_on_delete(&bucket_name, delete_bucket)
                                .await?
                        }
                        ModelStorageBindingSyncPolicyPush::Never => true,
                    };
                Ok(delete)
            }
            None => Ok(true),
        }
        .map_err(|error: Error| anyhow!("failed to unsync a bucket ({bucket_name}): {error}"))
    }

    async fn unsync_bucket_pull_always(
        &self,
        _source: &ObjectStorageRef,
        _bucket: &str,
    ) -> Result<bool> {
        dbg!("unsyncing on Pull=Always is not supported");
        Ok(true)
    }

    async fn unsync_bucket_push_always(
        &self,
        _source: &ObjectStorageRef,
        _bucket: &str,
    ) -> Result<bool> {
        dbg!("unsyncing on Push=Always is not supported");
        Ok(true)
    }

    async fn unsync_bucket_push_on_delete(
        &self,
        bucket: &str,
        delete_bucket: bool,
    ) -> Result<bool> {
        let (source_session, bucket) = self.inverted(bucket)?;
        let spec = BucketJobSpec {
            delete_source: delete_bucket,
            source: Some(self.target_ref),
            sync_source: true,
            ..Default::default()
        };
        source_session
            .get_or_create_bucket_job(bucket, "push", spec)
            .await?;
        Ok(false)
    }

    async fn get_or_create_bucket_job(
        &self,
        bucket: &str,
        command: &str,
        BucketJobSpec {
            delete_source,
            delete_target,
            source,
            sync_source,
            sync_source_overwrite,
        }: BucketJobSpec<'_>,
    ) -> Result<()> {
        let api = Api::<Job>::namespaced(self.kube.clone(), self.namespace);
        let name = format!(
            "{bucket}-{command}-{timestamp}",
            timestamp = Utc::now().timestamp(),
        );

        get_or_create(&api, "service", &name, || {
            let source_bucket = self.source_binding_name.unwrap_or(bucket).to_string();
            let target_bucket = bucket.to_string();

            let source_creds = source.as_ref().map(|source| source.provider.fetch());
            let target_creds = self.target_ref.provider.fetch();

            let source_endpoint = source.as_ref().map(|source| source.endpoint.to_string());
            let target_endpoint = self.target_ref.endpoint.to_string();

            let labels: BTreeMap<_, _> = vec![
                ("dash.ulagbulag.io/modelstorage.name", bucket),
                (
                    "dash.ulagbulag.io/modelstorage.objectstorage.command",
                    command,
                ),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();

            Job {
                metadata: ObjectMeta {
                    labels: Some(labels.clone()),
                    name: Some(name.clone()),
                    namespace: Some(self.namespace.to_string()),
                    ..Default::default()
                },
                spec: Some(JobSpec {
                    ttl_seconds_after_finished: Some(30),
                    template: PodTemplateSpec {
                        metadata: Some(ObjectMeta {
                            labels: Some(labels),
                            ..Default::default()
                        }),
                        spec: Some(PodSpec {
                            affinity: Some(Affinity {
                                node_affinity: Some(get_default_node_affinity()),
                                ..Default::default()
                            }),
                            containers: vec![Container {
                                name: "minio-client".into(),
                                image: Some("docker.io/minio/mc:latest".into()),
                                image_pull_policy: Some("Always".into()),
                                command: Some(vec![
                                    "/usr/bin/env".into(),
                                    "/bin/bash".into(),
                                    "-c".into(),
                                ]),
                                args: Some(vec![r#"
#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

# Add endpoints
echo '* Adding endpoints...'
if [ "x${SOURCE_ENDPOINT}" != 'x' ]; then
    mc alias set 'source' "${SOURCE_ENDPOINT}" "${SOURCE_ACCESS_KEY}" "${SOURCE_SECRET_KEY}"
fi
mc alias set 'target' "${TARGET_ENDPOINT}" "${TARGET_ACCESS_KEY}" "${TARGET_SECRET_KEY}"

# Sync
echo '* Starting sync...'
if [ "x${SOURCE_ENDPOINT}" != 'x' ]; then
    if [ "x${SOURCE_SYNC}" = 'xtrue' ]; then
        __ARGS=''
        if [ "x${SOURCE_SYNC_OVERWRITE}" = 'xtrue' ]; then
            __ARGS="${__ARGS} --overwrite"
        fi
        mc mirror "source/${SOURCE_BUCKET}" "target/${TARGET_BUCKET}" --quiet ${__ARGS}
    fi
fi

# Delete
echo '* Deleting buckets...'
if [ "x${SOURCE_ENDPOINT}" != 'x' ]; then
    if [ "x${SOURCE_DELETE_BUCKET}" = 'xtrue' ]; then
        mc rb "source/${SOURCE_BUCKET}" --force
    fi
fi
if [ "x${TARGET_DELETE_BUCKET}" = 'xtrue' ]; then
    mc rb "target/${TARGET_BUCKET}" --force
fi

# Finished!
exec true
"#
                                .into()]),
                                env: Some(vec![
                                    EnvVar {
                                        name: "SOURCE_ACCESS_KEY".into(),
                                        value: source_creds
                                            .as_ref()
                                            .map(|creds| creds.access_key.clone()),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "SOURCE_BUCKET".into(),
                                        value: Some(source_bucket),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "SOURCE_DELETE_BUCKET".into(),
                                        value: Some(delete_source.to_string()),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "SOURCE_ENDPOINT".into(),
                                        value: source_endpoint,
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "SOURCE_SECRET_KEY".into(),
                                        value: source_creds
                                            .as_ref()
                                            .map(|creds| creds.secret_key.clone()),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "SOURCE_SYNC".into(),
                                        value: Some(sync_source.to_string()),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "SOURCE_SYNC_OVERWRITE".into(),
                                        value: Some(sync_source_overwrite.to_string()),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "TARGET_ACCESS_KEY".into(),
                                        value: Some(target_creds.access_key),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "TARGET_BUCKET".into(),
                                        value: Some(target_bucket),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "TARGET_DELETE_BUCKET".into(),
                                        value: Some(delete_target.to_string()),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "TARGET_ENDPOINT".into(),
                                        value: Some(target_endpoint),
                                        value_from: None,
                                    },
                                    EnvVar {
                                        name: "TARGET_SECRET_KEY".into(),
                                        value: Some(target_creds.secret_key),
                                        value_from: None,
                                    },
                                ]),
                                ..Default::default()
                            }],
                            restart_policy: Some("OnFailure".into()),
                            ..Default::default()
                        }),
                    },
                    ..Default::default()
                }),
                status: None,
            }
        })
        .await?;
        Ok(())
    }
}

impl ObjectStorageRef {
    fn get_client(&self) -> ::minio::s3::client::Client<'_> {
        let mut client =
            ::minio::s3::client::Client::new(self.base_url.clone(), Some(&self.provider));
        client.ignore_cert_check = true;
        client
    }
}

struct MinioAdminClient<'storage> {
    storage: &'storage ObjectStorageRef,
}

impl<'storage> MinioAdminClient<'storage> {
    async fn add_site_replication(&self, target: &ObjectStorageRef) -> Result<()> {
        let origin_creds = self.storage.provider.fetch();
        let target_creds = target.provider.fetch();

        let sites = json! ([
            {
                "name": format!(
                    "{origin},{target},{name}",
                    name = &self.storage.name,
                    origin = &self.storage.endpoint,
                    target = &target.endpoint,
                ),
                "endpoints": &self.storage.endpoint,
                "accessKey": origin_creds.access_key,
                "secretKey": origin_creds.secret_key,
            },
            {
                "name": format!(
                    "{target},{origin},{name}",
                    name = &target.name,
                    origin = &self.storage.endpoint,
                    target = &target.endpoint,
                ),
                "endpoints": &target.endpoint,
                "accessKey": target_creds.access_key,
                "secretKey": target_creds.secret_key,
            },
        ]);
        let ciphertext = self.encrypt_data(Some(&origin_creds), &sites)?;

        self.execute::<&str>(
            Method::PUT,
            "/admin/v3/site-replication/add",
            &[],
            Some(&ciphertext),
        )
        .await
        .map(|_| ())
        .map_err(|error| {
            anyhow!(
                "failed to add site replication ({name}: {origin} => {target}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
                target = &target.endpoint,
            )
        })
    }

    async fn is_site_replication_enabled(&self) -> Result<bool> {
        self.execute::<&str>(Method::GET, "/admin/v3/site-replication/info", &[], None)
            .and_then(|resp| async move {
                #[derive(Deserialize)]
                struct Data {
                    enabled: bool,
                }

                let data: Data = resp.json().await?;
                Ok(data.enabled)
            })
            .await
            .map_err(|error| {
                anyhow!(
                    "failed to check site replication ({name}: {origin}): {error}",
                    name = &self.storage.name,
                    origin = &self.storage.endpoint,
                )
            })
    }

    #[allow(dead_code)]
    async fn list_remote_targets(&self, bucket_name: &str) -> Result<Vec<Map<String, Value>>> {
        self.execute(
            Method::GET,
            "/admin/v3/list-remote-targets",
            &[("type", "replication"), ("bucket", bucket_name)],
            None,
        )
        .and_then(|resp| async move {
            let targets = resp.json().await?;
            Ok(targets)
        })
        .await
        .map_err(|error| {
            anyhow!(
                "failed to list remote targets ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
    }

    #[allow(dead_code)]
    async fn remove_remote_target(&self, bucket_name: &str, arn: &str) -> Result<()> {
        self.execute(
            Method::DELETE,
            "/admin/v3/remove-remote-target",
            &[("arn", arn), ("bucket", bucket_name)],
            None,
        )
        .await
        .map(|_| ())
        .map_err(|error| {
            anyhow!(
                "failed to remove remote target ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
    }

    async fn set_remote_target(
        &self,
        target: &ObjectStorageRef,
        bucket_source: Option<&str>,
        bucket_target: &str,
    ) -> Result<String> {
        let origin_creds = self.storage.provider.fetch();
        let target_creds = target.provider.fetch();

        let site = json! ({
            "sourcebucket": bucket_target,
            "endpoint": target.endpoint.host_str(),
            "credentials": {
                "accessKey": target_creds.access_key,
                "secretKey": target_creds.secret_key,
            },
            "targetbucket": bucket_source.unwrap_or(bucket_target),
            "secure": target.endpoint.scheme() == "https",
            "type": "replication",
            "replicationSync": false,
            "disableProxy": false,
        });
        let ciphertext = self.encrypt_data(Some(&origin_creds), &site)?;

        self.execute(
            Method::PUT,
            "/admin/v3/set-remote-target",
            &[("bucket", bucket_target)],
            Some(&ciphertext),
        )
        .and_then(|resp| async move {
            let arn: String = resp.json().await?;
            Ok(arn)
        })
        .await
        .map_err(|error| {
            anyhow!(
                "failed to set remote target ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
    }

    async fn execute<Header>(
        &self,
        method: Method,
        base_url: &str,
        headers: &[(Header, Header)],
        data: Option<&[u8]>,
    ) -> Result<::reqwest::Response, ::minio::s3::error::Error>
    where
        Header: ToString,
    {
        let mut query_params = Multimap::default();
        for (key, value) in headers {
            query_params.insert(key.to_string(), value.to_string());
        }

        self.storage
            .get_client()
            .execute(
                method,
                &Default::default(),
                &mut Default::default(),
                &query_params,
                Some("minio"),
                Some(base_url),
                data,
            )
            .await
    }

    fn encrypt_data<T>(
        &self,
        creds: Option<&Credentials>,
        data: &T,
    ) -> Result<Vec<u8>, ::minio::s3::error::Error>
    where
        T: ?Sized + Serialize,
    {
        let creds = creds
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(self.storage.provider.fetch()));
        let data = ::serde_json::to_vec(&data)?;

        // FIXME: use CryptoRng instead!
        let mut rng = thread_rng();

        let mut salt = [0u8; 32];
        rng.fill(&mut salt);

        const ID: u8 = 0x01; // argon2idChaCHa20Poly1305
        let mut key = [0u8; 32];
        ::argon2::Argon2::new(
            Default::default(),
            Default::default(),
            ::argon2::Params::new(64 * 1024, 1, 4, Some(key.len())).unwrap(),
        )
        .hash_password_into(creds.secret_key.as_bytes(), &salt, &mut key)
        .unwrap();

        let mut nonce = [0u8; 8];
        rng.fill(&mut nonce);

        let mut encrypted_data = {
            // Load your secret keys from a secure location or derive
            // them using a secure (password-based) key-derivation-function, like Argon2id.
            // Obviously, don't use this all-zeros key for anything real.
            let key = ::sio::Key::<::sio::CHACHA20_POLY1305>::new(key);

            // Make sure you use an unique key-nonce combination!
            // Reusing a nonce value for the same secret key breaks
            // the security of the encryption algorithm.
            let nonce = ::sio::Nonce::new(nonce);

            // You must be able to re-generate this aad to decrypt
            // the ciphertext again. Usually, it's stored together with
            // the encrypted data.
            let aad = ::sio::Aad::empty();

            let mut buf = Vec::default(); // Store the ciphertext in memory.
            let mut writer = ::sio::EncWriter::new(&mut buf, &key, nonce, aad);

            writer.write_all(&data)?;
            writer.close()?; // Complete the encryption process explicitly.
            buf
        };

        // Prefix the ciphertext with salt, AEAD ID and nonce
        let mut ciphertext = Vec::new();
        ciphertext.append(&mut salt.to_vec());
        ciphertext.push(ID);
        ciphertext.append(&mut nonce.to_vec());
        ciphertext.append(&mut encrypted_data);
        Ok(ciphertext)
    }
}

async fn get_or_create_ingress(
    api: &Api<Ingress>,
    namespace: &str,
    name: &str,
    labels: Option<&BTreeMap<String, String>>,
    service: IngressServiceBackend,
) -> Result<Ingress> {
    get_or_create(api, "ingress", name, || Ingress {
        metadata: ObjectMeta {
            annotations: Some({
                let mut map = BTreeMap::default();
                map.insert(
                    "cert-manager.io/cluster-issuer".into(),
                    "ingress-nginx-controller.vine.svc.ops.openark".into(),
                );
                map.insert(
                    "kubernetes.io/ingress.class".into(),
                    "ingress-nginx-controller.vine.svc.ops.openark".into(),
                );
                map.insert(
                    "nginx.ingress.kubernetes.io/proxy-read-timeout".into(),
                    "3600".into(),
                );
                map.insert(
                    "nginx.ingress.kubernetes.io/proxy-send-timeout".into(),
                    "3600".into(),
                );
                map.insert(
                    "nginx.ingress.kubernetes.io/rewrite-target".into(),
                    "/$2".into(),
                );
                map.insert("vine.ulagbulag.io/is-service".into(), "true".into());
                map.insert("vine.ulagbulag.io/is-service-public".into(), "true".into());
                map.insert("vine.ulagbulag.io/is-service-system".into(), "true".into());
                map.insert(
                    "vine.ulagbulag.io/service-kind".into(),
                    "S3 Endpoint".into(),
                );
                map
            }),
            labels: labels.cloned(),
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: Some(IngressSpec {
            rules: Some(vec![IngressRule {
                host: Some("ingress-nginx-controller.vine.svc.ops.openark".into()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        path: Some(format!("/data/s3/{namespace}(/|$)(.*)")),
                        path_type: "Prefix".into(),
                        backend: IngressBackend {
                            service: Some(service),
                            ..Default::default()
                        },
                    }],
                }),
            }]),
            ..Default::default()
        }),
        ..Default::default()
    })
    .await
}

struct MinioTenantSpec<'a> {
    image: String,
    labels: &'a BTreeMap<String, String>,
    pool_name: &'a str,
    storage: &'a ModelStorageObjectOwnedSpec,
}

async fn get_or_create_minio_tenant(
    kube: &Client,
    namespace: &str,
    name: &str,
    MinioTenantSpec {
        image,
        labels,
        pool_name,
        storage:
            ModelStorageObjectOwnedSpec {
                minio_console_external_service,
                minio_external_service,
                replication:
                    ModelStorageObjectOwnedReplicationSpec {
                        total_nodes,
                        total_volumes_per_node,
                        resources,
                    },
                runtime_class_name,
                storage_class_name,
            },
    }: MinioTenantSpec<'_>,
) -> Result<Secret> {
    fn random_string(n: usize) -> String {
        let mut rng = thread_rng();
        (0..n).map(|_| rng.sample(Alphanumeric) as char).collect()
    }

    let api_secret = Api::<Secret>::namespaced(kube.clone(), namespace);
    let api_tenant = {
        let client = super::kubernetes::KubernetesStorageClient { namespace, kube };
        let spec = ModelCustomResourceDefinitionRefSpec {
            name: "tenants.minio.min.io/v2".into(),
        };
        client.api_custom_resource(&spec, None).await?
    };

    let total_volumes = total_nodes * total_volumes_per_node;
    let parity_level = total_volumes / 4; // >> 2
    let (
        ModelStorageObjectOwnedReplicationComputeResource(compute_resources),
        ModelStorageObjectOwnedReplicationStorageResource(storage_resources),
    ) = split_resources(resources, total_volumes)?;

    let secret_env_configuration = {
        let name = format!("{name}-env-configuration");
        get_or_create(&api_secret, "secret", &name, || Secret {
            metadata: ObjectMeta {
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            immutable: Some(false),
            string_data: Some({
                let mut map: BTreeMap<String, String> = BTreeMap::default();
                map.insert(
                    "config.env".into(),
                    format!(
                        r#"
export MINIO_BROWSER="on"
export MINIO_STORAGE_CLASS_STANDARD="EC:{parity_level}"
export MINIO_ROOT_USER="{username}"
export MINIO_ROOT_PASSWORD="{password}"
"#,
                        username = random_string(16),
                        password = random_string(32),
                    ),
                );
                map
            }),
            ..Default::default()
        })
        .await?
    };

    let secret_creds = {
        let name = format!("{name}-secret");
        get_or_create(&api_secret, "secret", &name, || Secret {
            metadata: ObjectMeta {
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            immutable: Some(true),
            string_data: Some({
                let mut map: BTreeMap<String, String> = BTreeMap::default();
                map.insert("accesskey".into(), Default::default());
                map.insert("secretkey".into(), Default::default());
                map
            }),
            ..Default::default()
        })
        .await?
    };

    let secret_user_0 = {
        let name = format!("{name}-user-0");
        get_or_create(&api_secret, "secret", &name, || Secret {
            metadata: ObjectMeta {
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            immutable: Some(true),
            string_data: Some({
                let mut map: BTreeMap<String, String> = BTreeMap::default();
                map.insert("CONSOLE_ACCESS_KEY".into(), random_string(16));
                map.insert("CONSOLE_SECRET_KEY".into(), random_string(32));
                map
            }),
            ..Default::default()
        })
        .await?
    };

    let users = [&secret_user_0]
        .iter()
        .map(|user| {
            json!({
                "name": user.name_any(),
            })
        })
        .collect::<Vec<_>>();

    get_or_create(&api_tenant, "tenant", name, || DynamicObject {
        types: Some(TypeMeta {
            api_version: "minio.min.io/v2".into(),
            kind: "Tenant".into(),
        }),
        metadata: ObjectMeta {
            labels: Some(labels.clone()),
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        data: json!({
            "spec": {
                "configuration": {
                    "name": secret_env_configuration.name_any(),
                },
                "credsSecret": {
                    "name": secret_creds.name_any(),
                },
                "exposeServices": {
                    "console": minio_console_external_service.is_enabled(),
                    "minio": minio_external_service.is_enabled(),
                },
                "image": image,
                "imagePullSecret": {},
                "mountPath": "/export",
                "pools": [
                    {
                        "affinity": {
                            "nodeAffinity": get_default_node_affinity(),
                            "podAntiAffinity": {
                                "requiredDuringSchedulingIgnoredDuringExecution": [
                                    {
                                        "labelSelector": {
                                            "matchExpressions": [
                                                {
                                                    "key": "v1.min.io/tenant",
                                                    "operator": "In",
                                                    "values": [
                                                        name,
                                                    ],
                                                },
                                                {
                                                    "key": "v1.min.io/pool",
                                                    "operator": "In",
                                                    "values": [
                                                        pool_name,
                                                    ],
                                                },
                                            ],
                                        },
                                        "topologyKey": "kubernetes.io/hostname",
                                    },
                                ],
                            },
                        },
                        "name": pool_name,
                        "resources": compute_resources,
                        "runtimeClassName": runtime_class_name,
                        "servers": total_nodes,
                        "volumeClaimTemplate": {
                            "metadata": {
                                "labels": labels,
                            },
                            "spec": {
                                "accessModes": [
                                    "ReadWriteOnce",
                                ],
                                "resources": storage_resources,
                                "storageClassName": storage_class_name,
                            },
                        },
                        "volumesPerServer": total_volumes_per_node,
                    },
                ],
                "requestAutoCert": false,
                "serviceMetadata": {
                    "consoleServiceAnnotations": {
                        "metallb.universe.tf/address-pool": minio_console_external_service.address_pool,
                        "metallb.universe.tf/loadBalancerIPs": minio_console_external_service.ip,
                    },
                    "minioServiceAnnotations": {
                        "metallb.universe.tf/address-pool": minio_external_service.address_pool,
                        "metallb.universe.tf/loadBalancerIPs": minio_external_service.ip,
                    },
                },
                "users": users,
            },
        }),
    })
    .await?;

    Ok(secret_user_0)
}

#[derive(Default)]
struct BucketJobSpec<'a> {
    delete_source: bool,
    delete_target: bool,
    source: Option<&'a ObjectStorageRef>,
    sync_source: bool,
    sync_source_overwrite: bool,
}

async fn get_or_create<K, Data>(api: &Api<K>, kind: &str, name: &str, data: Data) -> Result<K>
where
    Data: FnOnce() -> K,
    K: Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    match api.get_opt(name).await {
        Ok(Some(value)) => Ok(value),
        Ok(None) => {
            let pp = PostParams {
                dry_run: false,
                field_manager: Some(crate::NAME.into()),
            };
            api.create(&pp, &data())
                .await
                .map_err(|error| anyhow!("failed to create {kind} ({name}): {error}"))
        }
        Err(error) => bail!("failed to get {kind} ({name}): {error}"),
    }
}

fn split_resources(
    resources: &ResourceRequirements,
    total_volumes: u32,
) -> Result<(
    ModelStorageObjectOwnedReplicationComputeResource,
    ModelStorageObjectOwnedReplicationStorageResource,
)> {
    fn split_storage(
        compute_resources: &mut Option<BTreeMap<String, Quantity>>,
        total_volumes: u32,
        fill_default: bool,
    ) -> Result<Option<BTreeMap<String, Quantity>>> {
        let compute_resources = match compute_resources {
            Some(compute_resources) => compute_resources,
            None if fill_default => compute_resources.get_or_insert_with(Default::default),
            None => return Ok(None),
        };
        if fill_default {
            compute_resources.entry("cpu".into()).or_insert_with(|| {
                Quantity(ModelStorageObjectOwnedReplicationSpec::default_resources_cpu().into())
            });
            compute_resources.entry("memory".into()).or_insert_with(|| {
                Quantity(ModelStorageObjectOwnedReplicationSpec::default_resources_memory().into())
            });
        }

        let mut storage_resources = BTreeMap::default();
        let mut storage_resource = compute_resources.remove("storage");
        if fill_default {
            storage_resource.get_or_insert_with(|| {
                Quantity(ModelStorageObjectOwnedReplicationSpec::default_resources_storage().into())
            });
        }
        if let Some(storage_resource) = storage_resource {
            let storage_resource_as_bytes: Byte = storage_resource
                .0
                .parse()
                .map_err(|error| anyhow!("failed to parse storage volume size: {error}"))?;
            let storage_resource_per_volume =
                storage_resource_as_bytes.get_bytes() / total_volumes as u128;
            storage_resources.insert(
                "storage".into(),
                Quantity(storage_resource_per_volume.to_string()),
            );
        }
        Ok(Some(storage_resources))
    }

    let mut compute = resources.clone();
    let storage = ResourceRequirements {
        claims: compute.claims.clone(),
        limits: split_storage(&mut compute.limits, total_volumes, false)?,
        requests: split_storage(&mut compute.requests, total_volumes, true)?,
    };
    Ok((
        ModelStorageObjectOwnedReplicationComputeResource(compute),
        ModelStorageObjectOwnedReplicationStorageResource(storage),
    ))
}

fn get_default_node_affinity() -> NodeAffinity {
    NodeAffinity {
        preferred_during_scheduling_ignored_during_execution: Some(vec![
            PreferredSchedulingTerm {
                preference: NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss-ephemeral-control-plane".into(),
                        operator: "DoesNotExist".into(),
                        values: None,
                    }]),
                    match_fields: None,
                },
                weight: 1,
            },
            PreferredSchedulingTerm {
                preference: NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec!["Compute".into()]),
                    }]),
                    match_fields: None,
                },
                weight: 2,
            },
            PreferredSchedulingTerm {
                preference: NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec!["Gateway".into()]),
                    }]),
                    match_fields: None,
                },
                weight: 4,
            },
        ]),
        required_during_scheduling_ignored_during_execution: Some(NodeSelector {
            node_selector_terms: vec![NodeSelectorTerm {
                match_expressions: Some(vec![NodeSelectorRequirement {
                    key: "node-role.kubernetes.io/kiss".into(),
                    operator: "In".into(),
                    values: Some(vec![
                        "Compute".into(),
                        "ControlPlane".into(),
                        "Gateway".into(),
                    ]),
                }]),
                match_fields: None,
            }],
        }),
    }
}

async fn get_kubernetes_minio_domain(namespace: &str) -> Result<String> {
    Ok(format!(
        "minio.{namespace}.svc.{cluster_domain}",
        cluster_domain = get_cluster_domain().await?,
    ))
}

struct ModelStorageObjectOwnedReplicationComputeResource(ResourceRequirements);

struct ModelStorageObjectOwnedReplicationStorageResource(ResourceRequirements);
