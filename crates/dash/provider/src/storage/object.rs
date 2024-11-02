use std::{borrow::Cow, collections::BTreeMap, fmt, io::Write, net::IpAddr, str::FromStr};

use anyhow::{anyhow, bail, Error, Result};
use ark_core_k8s::data::Url;
use byte_unit::{Byte, UnitType};
use bytes::{BufMut, Bytes, BytesMut};
use chrono::Utc;
use dash_api::{
    model::{ModelCrd, ModelCustomResourceDefinitionRefSpec},
    model_storage_binding::{
        ModelStorageBindingStorageSourceSpec, ModelStorageBindingStorageSpec,
        ModelStorageBindingSyncPolicy, ModelStorageBindingSyncPolicyPull,
        ModelStorageBindingSyncPolicyPush,
    },
    model_user::ModelUserAccessTokenSecretRefSpec,
    storage::{
        object::{
            get_kubernetes_minio_endpoint, ModelStorageObjectBorrowedSpec,
            ModelStorageObjectClonedSpec, ModelStorageObjectOwnedReplicationSpec,
            ModelStorageObjectOwnedSpec, ModelStorageObjectRefSpec, ModelStorageObjectSpec,
        },
        ModelStorageCrd,
    },
};
use dash_provider_api::data::Capacity;
use futures::{stream::FuturesUnordered, FutureExt, TryFutureExt, TryStreamExt};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        batch::v1::{Job, JobSpec},
        core::v1::{
            Affinity, Capabilities, ConfigMap, Container, ContainerPort, EndpointAddress,
            EndpointPort, EndpointSubset, Endpoints, EnvVar, EnvVarSource, ExecAction,
            HTTPGetAction, Lifecycle, LifecycleHandler, NodeAffinity, NodeSelector,
            NodeSelectorRequirement, NodeSelectorTerm, ObjectFieldSelector, PodAffinity,
            PodAffinityTerm, PodSpec, PodTemplateSpec, PreferredSchedulingTerm, Probe,
            ResourceRequirements, Secret, SecurityContext, Service, ServiceAccount, ServicePort,
            ServiceSpec, WeightedPodAffinityTerm,
        },
        networking::v1::{
            HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressClass,
            IngressClassSpec, IngressRule, IngressServiceBackend, IngressSpec, ServiceBackendPort,
        },
        rbac::v1::{
            ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding, RoleRef, Subject,
        },
    },
    apimachinery::pkg::{
        api::resource::Quantity,
        apis::meta::v1::{LabelSelector, LabelSelectorRequirement, OwnerReference},
        util::intstr::IntOrString,
    },
};
use kube::{
    api::PostParams,
    core::{DynamicObject, ObjectMeta, TypeMeta},
    Api, Client, ResourceExt,
};
use maplit::btreemap;
use minio::s3::{
    args::{
        BucketExistsArgs, DeleteBucketReplicationArgs, GetBucketReplicationArgs, MakeBucketArgs,
        SetBucketReplicationArgs, SetBucketVersioningArgs,
    },
    creds::{Credentials, Provider, StaticProvider},
    http::BaseUrl,
    types::{Destination, ReplicationConfig, ReplicationRule, S3Api},
    utils::Multimap,
};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::try_join;
use tracing::{info, instrument, Level};

pub struct ObjectStorageClient {
    source: Option<(ObjectStorageSession, ModelStorageBindingSyncPolicy)>,
    source_binding_name: Option<String>,
    target: ObjectStorageSession,
}

impl ObjectStorageClient {
    #[instrument(level = Level::INFO, skip(kube, storage, metadata), err(Display))]
    pub async fn try_new<'source>(
        kube: &Client,
        namespace: &str,
        metadata: Option<&ObjectMeta>,
        storage: ModelStorageBindingStorageSpec<'source, &ModelStorageObjectSpec>,
        prometheus_url: Option<&str>,
    ) -> Result<Self> {
        Ok(Self {
            source: match storage.source {
                Some(ModelStorageBindingStorageSourceSpec {
                    name: source_name,
                    storage: source,
                    sync_policy,
                }) => Some(
                    ObjectStorageSession::load_storage_provider(
                        kube,
                        namespace,
                        source_name,
                        metadata,
                        source,
                        prometheus_url,
                    )
                    .await
                    .map(|source| (source, sync_policy))?,
                ),
                None => None,
            },
            source_binding_name: storage.source_binding_name.map(Into::into),
            target: ObjectStorageSession::load_storage_provider(
                kube,
                namespace,
                storage.target_name,
                metadata,
                storage.target,
                prometheus_url,
            )
            .await?,
        })
    }

    pub const fn target(&self) -> &ObjectStorageSession {
        &self.target
    }

    pub fn get_session<'model>(
        &self,
        kube: &'model Client,
        namespace: &'model str,
        model: &'model ModelCrd,
    ) -> ObjectStorageRef<'_, 'model, '_> {
        ObjectStorageRef {
            kube,
            model,
            namespace,
            source: self
                .source
                .as_ref()
                .map(|(source, sync_policy)| (source, *sync_policy)),
            source_binding_name: self.source_binding_name.as_deref(),
            target: &self.target,
        }
    }
}

pub struct ObjectStorageSession {
    pub client: ::minio::s3::client::Client,
    pub endpoint: Url,
    pub name: String,
    pub provider: StaticProvider,
}

impl fmt::Debug for ObjectStorageSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectStorageSession")
            .field("endpoint", &self.endpoint)
            .field("name", &self.name)
            .finish()
    }
}

impl<'model> ObjectStorageSession {
    #[instrument(level = Level::INFO, skip(kube, storage, metadata), err(Display))]
    pub async fn load_storage_provider(
        kube: &Client,
        namespace: &str,
        name: &str,
        metadata: Option<&ObjectMeta>,
        storage: &ModelStorageObjectSpec,
        prometheus_url: Option<&str>,
    ) -> Result<Self> {
        match storage {
            ModelStorageObjectSpec::Borrowed(storage) => {
                let _ = Self::create_or_get_storage(kube, namespace, metadata).await?;
                Self::load_storage_provider_by_borrowed(
                    kube,
                    namespace,
                    name,
                    storage,
                    prometheus_url,
                )
                .await
            }
            ModelStorageObjectSpec::Cloned(storage) => {
                let metadata = Self::create_or_get_storage(kube, namespace, metadata).await?;
                Self::load_storage_provider_by_cloned(
                    kube,
                    namespace,
                    name,
                    &metadata,
                    storage,
                    prometheus_url,
                )
                .await
            }
            ModelStorageObjectSpec::Owned(storage) => {
                let metadata = Self::create_or_get_storage(kube, namespace, metadata).await?;
                Self::load_storage_provider_by_owned(
                    kube,
                    namespace,
                    name,
                    &metadata,
                    storage,
                    prometheus_url,
                )
                .await
            }
        }
        .map_err(|error| anyhow!("failed to load object storage provider: {error}"))
    }

    #[instrument(level = Level::INFO, skip(kube, storage), err(Display))]
    async fn load_storage_provider_by_borrowed(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectBorrowedSpec,
        prometheus_url: Option<&str>,
    ) -> Result<Self> {
        let ModelStorageObjectBorrowedSpec { reference } = storage;
        Self::load_storage_provider_by_reference(kube, namespace, name, reference, prometheus_url)
            .await
    }

    #[instrument(level = Level::INFO, skip(kube, storage, metadata), err(Display))]
    async fn load_storage_provider_by_cloned(
        kube: &Client,
        namespace: &str,
        name: &str,
        metadata: &ObjectMeta,
        storage: &ModelStorageObjectClonedSpec,
        prometheus_url: Option<&str>,
    ) -> Result<Self> {
        let reference = Self::load_storage_provider_by_reference(
            kube,
            namespace,
            name,
            &storage.reference,
            prometheus_url.clone(),
        )
        .await?;
        let owned = Self::load_storage_provider_by_owned(
            kube,
            namespace,
            name,
            metadata,
            &storage.owned,
            prometheus_url,
        )
        .await?;

        let admin = MinioAdminClient {
            storage: &reference,
        };
        // TODO: verify and join endpoint
        if !admin.is_site_replication_enabled().await? {
            admin.add_site_replication(&owned).await?;
        }
        Ok(owned)
    }

    #[instrument(level = Level::INFO, skip(kube, storage, metadata), err(Display))]
    async fn load_storage_provider_by_owned(
        kube: &Client,
        namespace: &str,
        name: &str,
        metadata: &ObjectMeta,
        storage: &ModelStorageObjectOwnedSpec,
        prometheus_url: Option<&str>,
    ) -> Result<Self> {
        let storage = Self::create_or_get_minio_storage(
            kube,
            namespace,
            name,
            metadata,
            storage,
            prometheus_url,
        )
        .await?;
        Self::load_storage_provider_by_reference(kube, namespace, name, &storage, prometheus_url)
            .await
    }

    #[instrument(level = Level::INFO, skip(kube, storage), err(Display))]
    async fn load_storage_provider_by_reference(
        kube: &Client,
        namespace: &str,
        name: &str,
        storage: &ModelStorageObjectRefSpec,
        prometheus_url: Option<&str>,
    ) -> Result<Self> {
        let ModelStorageObjectRefSpec {
            endpoint,
            secret_ref:
                ModelUserAccessTokenSecretRefSpec {
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

        let base_url: BaseUrl = endpoint
            .as_str()
            .parse()
            .map_err(|error| anyhow!("failed to parse s3 storage endpoint: {error}"))?;
        let provider = StaticProvider::new(&access_key, &secret_key, None);
        let ssl_cert_file = None;
        let ignore_cert_check = Some(!base_url.https);

        Ok(Self {
            client: ::minio::s3::client::Client::new(
                base_url,
                Some(Box::new(provider.clone())),
                ssl_cert_file,
                ignore_cert_check,
            )?,
            endpoint: endpoint.clone(),
            name: name.to_string(),
            provider,
        })
    }

    #[must_use]
    #[instrument(level = Level::INFO, skip(kube, metadata), err(Display))]
    async fn create_or_get_storage(
        kube: &Client,
        namespace: &str,
        metadata: Option<&ObjectMeta>,
    ) -> Result<ObjectMeta> {
        async fn get_latest_nginx_controller_image() -> Result<String> {
            Ok("registry.k8s.io/ingress-nginx/controller:v1.10.0".into())
        }

        let api_service = Api::namespaced(kube.clone(), namespace);

        let tenant_name = get_default_tenant_name();
        let cluster_role_name = format!("dash:{tenant_name}");
        let ingress_class_controller = format!("k8s.io/dash/{tenant_name}/{namespace}");
        let ingress_class_name = get_ingress_class_name(namespace, tenant_name);
        let service_metrics_name = format!("{tenant_name}-metrics");
        let service_minio_name = "minio".into();

        let port_http_name = "http";
        let port_https_name = "https";
        let port_metrics_name = "metrics";

        let annotations = metadata
            .as_ref()
            .and_then(|metadata| metadata.annotations.clone());
        let mut labels = metadata
            .as_ref()
            .and_then(|metadata| metadata.labels.clone())
            .unwrap_or_default();
        labels.insert("app".into(), tenant_name.into());
        labels.insert(
            "dash.ulagbulag.io/modelstorage-type".into(),
            tenant_name.into(),
        );

        let service_type = match labels
            .get(ModelStorageCrd::LABEL_IS_EXTERNAL)
            .map(|label| label.as_str())
        {
            Some("true") => Some("LoadBalancer".into()),
            Some(_) | None => None,
        };

        let metadata = ObjectMeta {
            name: Some(tenant_name.into()),
            namespace: Some(namespace.into()),
            annotations,
            labels: Some(labels.clone()),
            ..Default::default()
        };

        let labels_service_metrics = {
            let mut labels = labels.clone();
            labels.insert(
                "dash.ulagbulag.io/modelstorage-service".into(),
                "metrics".into(),
            );
            labels
        };

        let uid = {
            let api = Api::namespaced(kube.clone(), namespace);
            let data = || ServiceAccount {
                metadata: metadata.clone(),
                ..Default::default()
            };
            let account = get_or_create(&api, "serviceaccount", tenant_name, data).await?;
            account
                .uid()
                .ok_or_else(|| anyhow!("failed to get serviceaccount uid"))?
        };

        let metadata = {
            let mut metadata = metadata;
            metadata.owner_references = Some(vec![OwnerReference {
                api_version: "v1".into(),
                block_owner_deletion: Some(true),
                controller: None,
                kind: "ServiceAccount".into(),
                name: tenant_name.into(),
                uid,
            }]);
            metadata
        };

        {
            let api = Api::namespaced(kube.clone(), namespace);
            let data = || Role {
                metadata: metadata.clone(),
                rules: Some(vec![
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec![
                            "configmaps".into(),
                            "endpoints".into(),
                            "pods".into(),
                            "secrets".into(),
                            "services".into(),
                        ]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec!["namespaces".into()]),
                        verbs: vec!["get".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec!["events".into()]),
                        verbs: vec!["create".into(), "patch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["coordination.k8s.io".into()]),
                        resource_names: Some(vec!["ingress-nginx-leader".into()]),
                        resources: Some(vec!["leases".into()]),
                        verbs: vec!["get".into(), "update".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["coordination.k8s.io".into()]),
                        resources: Some(vec!["leases".into()]),
                        verbs: vec!["create".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["discovery.k8s.io".into()]),
                        resources: Some(vec!["endpointslices".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["networking.k8s.io".into()]),
                        resources: Some(vec!["ingressclasses".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["networking.k8s.io".into()]),
                        resources: Some(vec!["ingresses".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["networking.k8s.io".into()]),
                        resources: Some(vec!["ingresses/status".into()]),
                        verbs: vec!["update".into()],
                        ..Default::default()
                    },
                ]),
            };
            get_or_create(&api, "role", tenant_name, data).await?
        };
        {
            let api = Api::namespaced(kube.clone(), namespace);
            let data = || RoleBinding {
                metadata: metadata.clone(),
                role_ref: RoleRef {
                    api_group: "rbac.authorization.k8s.io".into(),
                    kind: "Role".into(),
                    name: tenant_name.into(),
                },
                subjects: Some(vec![Subject {
                    api_group: Some(String::default()),
                    kind: "ServiceAccount".into(),
                    name: tenant_name.into(),
                    namespace: Some(namespace.into()),
                }]),
            };
            get_or_create(&api, "rolebinding", tenant_name, data).await?
        };
        {
            let api = Api::all(kube.clone());
            let data = || ClusterRole {
                aggregation_rule: None,
                metadata: ObjectMeta {
                    name: Some(cluster_role_name.clone()),
                    owner_references: None,
                    ..metadata.clone()
                },
                rules: Some(vec![
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec![
                            "configmaps".into(),
                            "endpoints".into(),
                            "namespaces".into(),
                            "nodes".into(),
                            "pods".into(),
                            "secrets".into(),
                            "services".into(),
                        ]),
                        verbs: vec!["list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec!["nodes".into()]),
                        verbs: vec!["get".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec!["services".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["".into()]),
                        resources: Some(vec!["events".into()]),
                        verbs: vec!["create".into(), "patch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["coordination.k8s.io".into()]),
                        resources: Some(vec!["leases".into()]),
                        verbs: vec!["list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["discovery.k8s.io".into()]),
                        resources: Some(vec!["endpointslices".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["networking.k8s.io".into()]),
                        resources: Some(vec!["ingressclasses".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["networking.k8s.io".into()]),
                        resources: Some(vec!["ingresses".into()]),
                        verbs: vec!["get".into(), "list".into(), "watch".into()],
                        ..Default::default()
                    },
                    PolicyRule {
                        api_groups: Some(vec!["networking.k8s.io".into()]),
                        resources: Some(vec!["ingresses/status".into()]),
                        verbs: vec!["update".into()],
                        ..Default::default()
                    },
                ]),
            };
            get_or_create(&api, "clusterrole", &cluster_role_name, data).await?
        };
        {
            let api = Api::all(kube.clone());
            let name = format!("{cluster_role_name}:{namespace}");
            let data = || ClusterRoleBinding {
                metadata: ObjectMeta {
                    name: Some(name.clone()),
                    ..metadata.clone()
                },
                role_ref: RoleRef {
                    api_group: "rbac.authorization.k8s.io".into(),
                    kind: "ClusterRole".into(),
                    name: cluster_role_name,
                },
                subjects: Some(vec![Subject {
                    api_group: Some(String::default()),
                    kind: "ServiceAccount".into(),
                    name: tenant_name.into(),
                    namespace: Some(namespace.into()),
                }]),
            };
            get_or_create(&api, "clusterrolebinding", &name, data).await?
        };
        {
            let api = Api::namespaced(kube.clone(), namespace);
            let data = || ConfigMap {
                metadata: metadata.clone(),
                immutable: Some(true),
                binary_data: None,
                data: Some(btreemap! {
                    "allow-snippet-annotations".into() => "true".into(),
                }),
            };
            get_or_create(&api, "configmap", tenant_name, data).await?
        };
        {
            let image = get_latest_nginx_controller_image().await?;

            let api = Api::namespaced(kube.clone(), namespace);
            let data = || Deployment {
                metadata: metadata.clone(),
                spec: Some(DeploymentSpec {
                    selector: LabelSelector {
                        match_expressions: None,
                        match_labels: Some(labels.clone()),
                    },
                    template: PodTemplateSpec {
                        metadata: Some(ObjectMeta {
                            annotations: Some(btreemap! {
                                "prometheus.io/port".into() => "10254".into(),
                                "prometheus.io/scrape".into() => "true".into(),
                            }),
                            labels: Some(labels.clone()),
                            ..Default::default()
                        }),
                        spec: Some(PodSpec {
                            affinity: Some(Affinity {
                                node_affinity: Some(get_default_node_affinity()),
                                pod_affinity: Some(get_default_pod_affinity(tenant_name)),
                                ..Default::default()
                            }),
                            containers: vec![Container {
                                name: "controller".into(),
                                image: Some(image),
                                image_pull_policy: Some("Always".into()),
                                args: Some(vec![
                                    "/nginx-ingress-controller".into(),
                                    format!("--publish-service=$(POD_NAMESPACE)/{tenant_name}"),
                                    "--election-id=ingress-nginx-leader".into(),
                                    format!("--controller-class={ingress_class_controller}"),
                                    format!("--ingress-class={ingress_class_name}"),
                                    format!("--configmap=$(POD_NAMESPACE)/{tenant_name}"),
                                ]),
                                env: Some(vec![
                                    EnvVar {
                                        name: "POD_NAME".into(),
                                        value: None,
                                        value_from: Some(EnvVarSource {
                                            field_ref: Some(ObjectFieldSelector {
                                                api_version: Some("v1".into()),
                                                field_path: "metadata.name".into(),
                                            }),
                                            ..Default::default()
                                        }),
                                    },
                                    EnvVar {
                                        name: "POD_NAMESPACE".into(),
                                        value: None,
                                        value_from: Some(EnvVarSource {
                                            field_ref: Some(ObjectFieldSelector {
                                                api_version: Some("v1".into()),
                                                field_path: "metadata.namespace".into(),
                                            }),
                                            ..Default::default()
                                        }),
                                    },
                                    EnvVar {
                                        name: "LD_PRELOAD".into(),
                                        value: Some("/usr/local/lib/libmimalloc.so".into()),
                                        value_from: None,
                                    },
                                ]),
                                lifecycle: Some(Lifecycle {
                                    pre_stop: Some(LifecycleHandler {
                                        exec: Some(ExecAction {
                                            command: Some(vec!["/wait-shutdown".into()]),
                                        }),
                                        ..Default::default()
                                    }),
                                    post_start: None,
                                }),
                                liveness_probe: Some(Probe {
                                    failure_threshold: Some(5),
                                    http_get: Some(HTTPGetAction {
                                        path: Some("/healthz".into()),
                                        port: IntOrString::Int(10254),
                                        scheme: Some("HTTP".into()),
                                        ..Default::default()
                                    }),
                                    initial_delay_seconds: Some(10),
                                    period_seconds: Some(10),
                                    success_threshold: Some(1),
                                    timeout_seconds: Some(1),
                                    ..Default::default()
                                }),
                                readiness_probe: Some(Probe {
                                    failure_threshold: Some(3),
                                    http_get: Some(HTTPGetAction {
                                        path: Some("/healthz".into()),
                                        port: IntOrString::Int(10254),
                                        scheme: Some("HTTP".into()),
                                        ..Default::default()
                                    }),
                                    initial_delay_seconds: Some(10),
                                    period_seconds: Some(10),
                                    success_threshold: Some(1),
                                    timeout_seconds: Some(1),
                                    ..Default::default()
                                }),
                                ports: Some(vec![
                                    ContainerPort {
                                        name: Some(port_http_name.into()),
                                        protocol: Some("TCP".into()),
                                        container_port: 80,
                                        ..Default::default()
                                    },
                                    ContainerPort {
                                        name: Some(port_https_name.into()),
                                        protocol: Some("TCP".into()),
                                        container_port: 443,
                                        ..Default::default()
                                    },
                                    ContainerPort {
                                        name: Some(port_metrics_name.into()),
                                        protocol: Some("TCP".into()),
                                        container_port: 10254,
                                        ..Default::default()
                                    },
                                ]),
                                resources: Some(ResourceRequirements {
                                    limits: Some(btreemap! {
                                        "cpu".into() => Quantity(ModelStorageObjectOwnedReplicationSpec::default_resources_cpu().into()),
                                        "memory".into() => Quantity(ModelStorageObjectOwnedReplicationSpec::default_resources_memory().into()),
                                    }),
                                    ..Default::default()
                                }),
                                security_context: Some(SecurityContext {
                                    allow_privilege_escalation: Some(false),
                                    capabilities: Some(Capabilities {
                                        add: Some(vec!["NET_BIND_SERVICE".into()]),
                                        drop: Some(vec!["ALL".into()]),
                                    }),
                                    read_only_root_filesystem: Some(false),
                                    run_as_non_root: Some(true),
                                    run_as_user: Some(101),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            }],
                            service_account_name: Some(tenant_name.into()),
                            ..Default::default()
                        }),
                    },
                    ..Default::default()
                }),
                status: None,
            };
            get_or_create(&api, "deployment", tenant_name, data).await?
        };
        {
            let api = &api_service;
            let data = || Service {
                metadata: metadata.clone(),
                spec: Some(ServiceSpec {
                    selector: Some(labels.clone()),
                    type_: service_type,
                    ports: Some(vec![
                        ServicePort {
                            name: Some(port_http_name.into()),
                            port: 80,
                            ..Default::default()
                        },
                        ServicePort {
                            name: Some(port_https_name.into()),
                            port: 443,
                            ..Default::default()
                        },
                    ]),
                    ..Default::default()
                }),
                status: None,
            };
            get_or_create(api, "service", tenant_name, data).await?
        };
        {
            let api = &api_service;
            let data = || Service {
                metadata: ObjectMeta {
                    labels: Some(labels_service_metrics.clone()),
                    name: Some(service_metrics_name.clone()),
                    ..metadata.clone()
                },
                spec: Some(ServiceSpec {
                    selector: Some(labels),
                    ports: Some(vec![ServicePort {
                        name: Some(port_metrics_name.into()),
                        port: 10254,
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                status: None,
            };
            get_or_create(api, "service", &service_metrics_name, data).await?
        };
        {
            let api = load_api_service_monitor(kube, namespace).await?;
            let data = || DynamicObject {
                types: Some(TypeMeta {
                    api_version: "monitoring.coreos.com/v1".into(),
                    kind: "ServiceMonitor".into(),
                }),
                metadata: metadata.clone(),
                data: json!({
                    "spec": {
                        "endpoints": [
                            {
                                "port": "metrics",
                                "interval": "30s",
                            },
                        ],
                        "namespaceSelector": {
                            "matchNames": [
                                namespace,
                            ],
                        },
                        "selector": {
                            "matchLabels": labels_service_metrics,
                        },
                    },
                }),
            };
            get_or_create(&api, "servicemonitor", tenant_name, data).await?
        };
        {
            let api = Api::namespaced(kube.clone(), namespace);
            let data = || Ingress {
                metadata: ObjectMeta {
                    annotations: Some(get_default_ingress_annotations()),
                    ..metadata.clone()
                },
                spec: Some(IngressSpec {
                    default_backend: None,
                    ingress_class_name: Some(ingress_class_name.clone()),
                    tls: None,
                    rules: Some(vec![IngressRule {
                        host: None,
                        http: Some(HTTPIngressRuleValue {
                            paths: vec![HTTPIngressPath {
                                path: Some("/".into()),
                                path_type: "Prefix".into(),
                                backend: IngressBackend {
                                    resource: None,
                                    service: Some(IngressServiceBackend {
                                        name: service_minio_name,
                                        port: Some(ServiceBackendPort {
                                            name: Some("http-minio".into()),
                                            number: None,
                                        }),
                                    }),
                                },
                            }],
                        }),
                    }]),
                }),
                status: None,
            };
            get_or_create(&api, "ingress", tenant_name, data).await?
        };
        {
            let api = Api::all(kube.clone());
            let data = || IngressClass {
                metadata: ObjectMeta {
                    annotations: Some(btreemap! {
                        "ingressclass.kubernetes.io/is-default-class".into() => "false".into(),
                    }),
                    name: Some(ingress_class_name.clone()),
                    ..metadata.clone()
                },
                spec: Some(IngressClassSpec {
                    controller: Some(ingress_class_controller),
                    parameters: None,
                }),
            };
            get_or_create(&api, "ingressclass", &ingress_class_name, data).await?
        };
        Ok(metadata)
    }

    #[instrument(level = Level::INFO, skip(kube, storage, metadata), err(Display))]
    async fn create_or_get_minio_storage(
        kube: &Client,
        namespace: &str,
        name: &str,
        metadata: &ObjectMeta,
        storage: &ModelStorageObjectOwnedSpec,
        prometheus_url: Option<&str>,
    ) -> Result<ModelStorageObjectRefSpec> {
        async fn get_latest_minio_image() -> Result<String> {
            Ok("docker.io/minio/minio:RELEASE.2024-08-03T04-33-23Z".into())
        }

        let tenant_name = get_default_tenant_name();
        let annotations = BTreeMap::default();
        let labels = btreemap! {
            "app".into() => "minio".into(),
            "dash.ulagbulag.io/modelstorage-name".into() => name.into(),
            "dash.ulagbulag.io/modelstorage-type".into() => tenant_name.into(),
            "v1.min.io/tenant".into() => tenant_name.into(),
        };

        let secret_user_0 = {
            let name = tenant_name;
            let pool_name = "pool-0";

            let spec = MinioTenantSpec {
                image: get_latest_minio_image().await?,
                annotations: &annotations,
                labels: &labels,
                owner_references: metadata.owner_references.as_ref(),
                pool_name,
                prometheus_url,
                storage,
            };
            get_or_create_minio_tenant(kube, namespace, name, spec).await?
        };

        Ok(ModelStorageObjectRefSpec {
            endpoint: get_kubernetes_minio_endpoint(namespace)
                .ok_or_else(|| anyhow!("failed to get minio storage endpoint"))?,
            secret_ref: ModelUserAccessTokenSecretRefSpec {
                map_access_key: "CONSOLE_ACCESS_KEY".into(),
                map_secret_key: "CONSOLE_SECRET_KEY".into(),
                name: secret_user_0.name_any(),
            },
        })
    }

    pub fn fetch_provider(&self) -> Credentials {
        self.provider.fetch()
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn get_capacity_global(&self) -> Result<Capacity> {
        let admin = MinioAdminClient { storage: self };
        admin.get_capacity_global().await
    }
}

pub struct ObjectStorageRef<'client, 'model, 'source> {
    kube: &'model Client,
    model: &'model ModelCrd,
    namespace: &'model str,
    source: Option<(&'source ObjectStorageSession, ModelStorageBindingSyncPolicy)>,
    source_binding_name: Option<&'client str>,
    target: &'source ObjectStorageSession,
}

impl<'client, 'model, 'source> ObjectStorageRef<'client, 'model, 'source> {
    fn get_bucket_name(&self) -> String {
        self.model.name_any()
    }

    fn admin(&self) -> MinioAdminClient<'_> {
        MinioAdminClient {
            storage: self.target,
        }
    }

    fn inverted(&self, bucket: &'client str) -> Result<(Self, &'client str)>
    where
        'source: 'client,
    {
        match self.source.as_ref() {
            Some((source, sync_policy)) => Ok((
                Self {
                    kube: self.kube,
                    model: self.model,
                    namespace: self.namespace,
                    source: Some((self.target, *sync_policy)),
                    source_binding_name: Some(bucket),
                    target: source,
                },
                self.source_binding_name.unwrap_or(bucket),
            )),
            None => bail!("cannot invert object storage session without source storage"),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn is_bucket_exists(&self) -> Result<bool> {
        let bucket_name = self.get_bucket_name();
        self.target
            .client
            .bucket_exists(&BucketExistsArgs::new(&bucket_name)?)
            .await
            .map_err(|error| anyhow!("failed to check bucket ({bucket_name}): {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get(&self, ref_name: &str) -> Result<Option<Value>> {
        let bucket_name = self.get_bucket_name();

        match self
            .target
            .client
            .get_object(&bucket_name, ref_name)
            .send()
            .await
        {
            Ok(response) => {
                response
                    .content
                    .to_stream()
                    .and_then(|(stream, _size)| stream.try_collect().map_err(Into::into))
                    .map(|result| {
                        result.and_then(|bytes: BytesMut| {
                            ::serde_json::from_slice(&bytes).map_err(Into::into)
                        })
                    })
                    .map_err(|error| {
                        anyhow!("failed to parse object ({bucket_name}/{ref_name}): {error}")
                    })
                    .await
            }
            Err(error) => match &error {
                ::minio::s3::error::Error::S3Error(response) if response.code == "NoSuchKey" => {
                    Ok(None)
                }
                _ => bail!("failed to get object ({bucket_name}/{ref_name}): {error}"),
            },
        }
    }

    pub async fn get_capacity(&self) -> Result<Capacity> {
        let admin = self.admin();
        let global_capacity = admin.get_capacity_global().await?;

        let bucket_name = self.get_bucket_name();
        match admin
            .get_capacity_bucket(&bucket_name)
            .await
            .unwrap_or_else(|error| {
                info!("failed to get bucket capacity: {error}");
                None
            }) {
            Some(bucket_capacity) => Ok(bucket_capacity.limit_on(global_capacity.capacity)),
            None => Ok(global_capacity),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get_list(&self) -> Result<Vec<Value>> {
        let bucket_name = self.get_bucket_name();

        match self
            .target
            .client
            .list_objects_v2(&bucket_name)
            .send()
            .await
        {
            Ok(response) => response
                .contents
                .into_iter()
                .map(|item| async move { self.get(&item.name).await })
                .collect::<FuturesUnordered<_>>()
                .try_collect()
                .await
                .map(|values: Vec<_>| values.into_iter().flatten().collect())
                .map_err(|error| anyhow!("failed to list object ({bucket_name}): {error}")),
            Err(error) => bail!("failed to list object ({bucket_name}): {error}"),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn create_bucket(
        &self,
        owner_references: Vec<OwnerReference>,
        quota: Option<Byte>,
    ) -> Result<()> {
        let mut bucket_name = self.get_bucket_name();
        if !self.is_bucket_exists().await? {
            let args = MakeBucketArgs::new(&bucket_name)?;
            bucket_name = match self.target.client.make_bucket(&args).await {
                Ok(response) => response.bucket_name,
                Err(error) => bail!("failed to create a bucket ({bucket_name}): {error}"),
            };
        }

        if let Some(quota) = quota {
            self.set_bucket_quota(&bucket_name, quota).await?;
        }
        self.sync_bucket(bucket_name).await?;
        self.create_bucket_service(owner_references).await
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn create_bucket_service(&self, owner_references: Vec<OwnerReference>) -> Result<()> {
        let bucket_name = self.get_bucket_name();
        let tenant_name = get_default_tenant_name();
        let name = bucket_name.to_string();

        let service_http_name = "http";

        let session = match self.source {
            Some((storage, policy)) if policy.is_none() => storage,
            Some(_) | None => self.target,
        };
        let external_name = session
            .endpoint
            .host_str()
            .ok_or_else(|| anyhow!("unknown endpoint host"))?;

        let service_ports = || {
            vec![ServicePort {
                name: Some(service_http_name.into()),
                protocol: Some("TCP".into()),
                port: 80,
                ..Default::default()
            }]
        };

        let labels = btreemap! {
            "dash.ulagbulag.io/model-name".into() => bucket_name.clone(),
            "dash.ulagbulag.io/modelstorage-name".into() => self.target.name.clone(),
            "dash.ulagbulag.io/modelstorage-type".into() => tenant_name.into(),
        };
        let metadata = ObjectMeta {
            name: Some(name.clone()),
            namespace: Some(self.namespace.into()),
            labels: Some(labels),
            owner_references: Some(owner_references),
            ..Default::default()
        };

        let service_spec = match IpAddr::from_str(external_name) {
            Ok(addr) => {
                let api = Api::namespaced(self.kube.clone(), self.namespace);
                let data = || Endpoints {
                    metadata: metadata.clone(),
                    subsets: Some(vec![EndpointSubset {
                        addresses: Some(vec![EndpointAddress {
                            ip: addr.to_string(),
                            ..Default::default()
                        }]),
                        not_ready_addresses: None,
                        ports: Some(vec![EndpointPort {
                            name: Some(service_http_name.into()),
                            protocol: Some("TCP".into()),
                            port: 80,
                            ..Default::default()
                        }]),
                    }]),
                };
                get_or_create(&api, "service", &name, data).await?;

                ServiceSpec {
                    ports: Some(service_ports()),
                    ..Default::default()
                }
            }
            Err(_) => ServiceSpec {
                type_: Some("ExternalName".into()),
                external_name: Some(external_name.into()),
                ports: Some(service_ports()),
                ..Default::default()
            },
        };
        {
            let api = Api::namespaced(self.kube.clone(), self.namespace);
            let data = || Service {
                metadata: metadata.clone(),
                spec: Some(service_spec),
                status: None,
            };
            get_or_create(&api, "service", &name, data).await?
        };
        {
            let api = Api::namespaced(self.kube.clone(), self.namespace);
            let ingress_class_name = get_ingress_class_name(self.namespace, tenant_name);
            let data = || Ingress {
                metadata: ObjectMeta {
                    annotations: Some(get_default_ingress_annotations()),
                    ..metadata.clone()
                },
                spec: Some(IngressSpec {
                    default_backend: None,
                    ingress_class_name: Some(ingress_class_name.clone()),
                    tls: None,
                    rules: Some(vec![IngressRule {
                        host: None,
                        http: Some(HTTPIngressRuleValue {
                            paths: vec![
                                HTTPIngressPath {
                                    path: Some(format!("/{bucket_name}")),
                                    path_type: "Exact".into(),
                                    backend: IngressBackend {
                                        resource: None,
                                        service: Some(IngressServiceBackend {
                                            name: name.clone(),
                                            port: Some(ServiceBackendPort {
                                                name: Some(service_http_name.into()),
                                                number: None,
                                            }),
                                        }),
                                    },
                                },
                                HTTPIngressPath {
                                    path: Some(format!("/{bucket_name}/")),
                                    path_type: "Prefix".into(),
                                    backend: IngressBackend {
                                        resource: None,
                                        service: Some(IngressServiceBackend {
                                            name: name.clone(),
                                            port: Some(ServiceBackendPort {
                                                name: Some(service_http_name.into()),
                                                number: None,
                                            }),
                                        }),
                                    },
                                },
                            ],
                        }),
                    }]),
                }),
                status: None,
            };
            get_or_create(&api, "ingress", &name, data).await?
        };
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn set_bucket_quota(&self, bucket_name: &str, quota: Byte) -> Result<()> {
        self.admin().set_capacity_bucket(bucket_name, quota).await
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
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

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn sync_bucket_pull_always(&self, bucket: &str) -> Result<()> {
        self.target
            .client
            .set_bucket_versioning(&SetBucketVersioningArgs::new(bucket, true)?)
            .await
            .map_err(|error| {
                anyhow!("failed to enable bucket versioning for Pulling ({bucket}): {error}")
            })?;

        let (source_session, bucket) = self.inverted(bucket)?;
        let source = self.target;
        source_session.sync_bucket_push_always(source, bucket).await
    }

    #[instrument(level = Level::INFO, skip(self, source), err(Display))]
    async fn sync_bucket_pull_on_create(
        &self,
        source: &ObjectStorageSession,
        bucket: &str,
    ) -> Result<()> {
        let spec = BucketJobSpec {
            source: Some(source),
            sync_source: true,
            ..Default::default()
        };
        self.get_or_create_bucket_job(bucket, "pull", spec).await
    }

    #[instrument(level = Level::INFO, skip(self, source), err(Display))]
    async fn sync_bucket_push_always(
        &self,
        source: &ObjectStorageSession,
        bucket: &str,
    ) -> Result<()> {
        self.target
            .client
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
            .client
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
            .client
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

    #[instrument(level = Level::INFO, skip(self), err(Display))]
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

    #[instrument(level = Level::INFO, skip(self), err(Display))]
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
                        self.unsync_bucket_pull_always(&bucket_name).await?
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

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn unsync_bucket_pull_always(&self, bucket: &str) -> Result<bool> {
        let (source_session, bucket) = self.inverted(bucket)?;
        let source = self.target;
        source_session
            .unsync_bucket_push_always(source, bucket)
            .await
    }

    #[instrument(level = Level::INFO, skip(self, source), err(Display))]
    async fn unsync_bucket_push_always(
        &self,
        source: &ObjectStorageSession,
        bucket: &str,
    ) -> Result<bool> {
        let bucket_arn = match self
            .admin()
            .get_remote_target(source, self.source_binding_name, bucket)
            .await?
        {
            Some(arn) => arn,
            None => return Ok(true),
        };

        self.target
            .client
            .delete_bucket_replication(&DeleteBucketReplicationArgs {
                extra_headers: None,
                extra_query_params: None,
                region: None,
                bucket,
            })
            .await?;

        self.admin().remove_remote_target(bucket, &bucket_arn).await;
        Ok(true)
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn unsync_bucket_push_on_delete(
        &self,
        bucket: &str,
        delete_bucket: bool,
    ) -> Result<bool> {
        let (source_session, bucket) = self.inverted(bucket)?;
        let spec = BucketJobSpec {
            delete_source: delete_bucket,
            source: Some(self.target),
            sync_source: true,
            ..Default::default()
        };
        source_session
            .get_or_create_bucket_job(bucket, "push", spec)
            .await?;
        Ok(false)
    }

    #[instrument(level = Level::INFO, skip_all, fields(bucket = %bucket, command = %command), err(Display))]
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

        get_or_create(&api, "job", &name, || {
            let source_bucket = self.source_binding_name.unwrap_or(bucket).to_string();
            let target_bucket = bucket.to_string();

            let source_creds = source.as_ref().map(|source| source.provider.fetch());
            let target_creds = self.target.provider.fetch();

            let source_endpoint = source.as_ref().map(|source| source.endpoint.to_string());
            let target_endpoint = self.target.endpoint.to_string();

            let tenant_name = get_default_tenant_name();
            let labels = btreemap! {
                "dash.ulagbulag.io/modelstorage-name".into() => bucket.into(),
                "dash.ulagbulag.io/modelstorage-objectstorage-command".into() => command.into(),
                "dash.ulagbulag.io/modelstorage-type".into() => tenant_name.into(),
            };

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
                                pod_affinity: Some(get_default_pod_affinity(tenant_name)),
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

struct MinioAdminClient<'storage> {
    storage: &'storage ObjectStorageSession,
}

impl<'storage> MinioAdminClient<'storage> {
    #[instrument(level = Level::INFO, skip(self, target), err(Display))]
    async fn add_site_replication(&self, target: &ObjectStorageSession) -> Result<()> {
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
            Some(ciphertext),
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

    async fn get_capacity_bucket(&self, bucket_name: &str) -> Result<Option<Capacity>> {
        match try_join!(
            self.get_capacity_bucket_capacity(bucket_name),
            self.get_capacity_bucket_usage(bucket_name),
        )? {
            (Some(capacity), Some(usage)) => Ok(Some(Capacity { capacity, usage })),
            _ => Ok(None),
        }
    }

    async fn get_capacity_bucket_capacity(&self, bucket_name: &str) -> Result<Option<Byte>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Data {
            #[serde(default)]
            quota: Option<Byte>,
        }

        self.execute::<&str>(
            Method::GET,
            "/admin/v3/get-bucket-quota",
            &[("bucket", bucket_name)],
            None,
        )
        .map_err(Error::from)
        .map(|result| result.and_then(|bytes| ::serde_json::from_slice(&bytes).map_err(Into::into)))
        .map_ok(|data: Data| data.quota)
        .map_err(|error| {
            anyhow!(
                "failed to get total bucket capacity ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
        .await
    }

    async fn get_capacity_bucket_usage(&self, _bucket_name: &str) -> Result<Option<Byte>> {
        // NOTE(2023-11-12): minio API does not provide bucket usage in O(1)
        Ok(None)
    }

    async fn get_capacity_global(&self) -> Result<Capacity> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Data {
            pools: BTreeMap<String, BTreeMap<String, DataPool>>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DataPool {
            raw_capacity: u64,
            usage: u64,
        }

        self.execute::<&str>(Method::GET, "/admin/v3/info", &[], None)
            .map_err(Error::from)
            .map(|result| {
                result.and_then(|bytes| ::serde_json::from_slice(&bytes).map_err(Into::into))
            })
            .map_ok(|data: Data| {
                data.pools
                    .into_values()
                    .flatten()
                    .map(|(_, pool)| Capacity {
                        capacity: Byte::from_u64(pool.raw_capacity),
                        usage: Byte::from_u64(pool.usage),
                    })
                    .sum()
            })
            .map_err(|error| {
                anyhow!(
                    "failed to get available storage capacity ({name}: {origin}): {error}",
                    name = &self.storage.name,
                    origin = &self.storage.endpoint,
                )
            })
            .await
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn is_site_replication_enabled(&self) -> Result<bool> {
        self.execute::<&str>(Method::GET, "/admin/v3/site-replication/info", &[], None)
            .map_err(Error::from)
            .map(|result| {
                result.and_then(|bytes| {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Data {
                        enabled: bool,
                    }

                    ::serde_json::from_slice(&bytes)
                        .map(|data: Data| data.enabled)
                        .map_err(Into::into)
                })
            })
            .map_err(|error| {
                anyhow!(
                    "failed to check site replication ({name}: {origin}): {error}",
                    name = &self.storage.name,
                    origin = &self.storage.endpoint,
                )
            })
            .await
    }

    #[instrument(level = Level::INFO, skip(self, target), err(Display))]
    async fn get_remote_target(
        &self,
        target: &ObjectStorageSession,
        bucket_source: Option<&str>,
        bucket_target: &str,
    ) -> Result<Option<String>> {
        let bucket_source = bucket_source.unwrap_or(bucket_target);
        let target_creds = target.provider.fetch();

        let targets = self.list_remote_targets(bucket_target).await?;
        Ok(targets
            .into_iter()
            .find(|item| {
                Some(item.endpoint.as_str()) == target.endpoint.host_str()
                    && item.credentials.access_key == target_creds.access_key
                    && item.sourcebucket == bucket_source
                    && item.targetbucket == bucket_target
            })
            .map(|item| item.arn))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn list_remote_targets(&self, bucket_name: &str) -> Result<Vec<RemoteTarget>> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RemoteTargetList {
            Some(Vec<RemoteTarget>),
            None(()),
        }

        self.execute(
            Method::GET,
            "/admin/v3/list-remote-targets",
            &[("type", "replication"), ("bucket", bucket_name)],
            None,
        )
        .map(|result| {
            result.and_then(|bytes| {
                ::serde_json::from_slice(&bytes)
                    .map(|list| match list {
                        RemoteTargetList::Some(list) => list,
                        RemoteTargetList::None(()) => Vec::default(),
                    })
                    .map_err(Into::into)
            })
        })
        .map_err(|error| {
            anyhow!(
                "failed to list remote targets ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
        .await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn remove_remote_target(&self, bucket_name: &str, arn: &str) {
        self.execute(
            Method::PUT,
            "/admin/v3/remove-remote-target",
            &[("arn", arn), ("bucket", bucket_name)],
            None,
        )
        .await
        .ok();
    }

    async fn set_capacity_bucket(&self, bucket_name: &str, quota: Byte) -> Result<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct BucketQuota {
            /// Deprecated @ Aug 2023
            quota: u64,
            size: u64,
            #[serde(rename = "quotatype")]
            quota_type: QuotaType,
        }

        #[derive(Default, Serialize)]
        #[serde(rename_all = "camelCase")]
        enum QuotaType {
            #[default]
            Hard,
        }

        info!(
            "Setting bucket capacity ({name}: {origin}): {quota}",
            name = &self.storage.name,
            origin = &self.storage.endpoint,
            quota = quota.get_appropriate_unit(UnitType::Binary),
        );
        let data = BucketQuota {
            quota: quota.as_u64(),
            size: quota.as_u64(),
            quota_type: QuotaType::Hard,
        };
        let ciphertext = ::serde_json::to_vec(&data)?.into();

        self.execute::<&str>(
            Method::PUT,
            "/admin/v3/set-bucket-quota",
            &[("bucket", bucket_name)],
            Some(ciphertext),
        )
        .map_ok(|_| ())
        .map_err(|error| {
            anyhow!(
                "failed to set bucket capacity ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
        .await
    }

    #[instrument(level = Level::INFO, skip(self, target), err(Display))]
    async fn set_remote_target(
        &self,
        target: &ObjectStorageSession,
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
            Some(ciphertext),
        )
        .map(|result| result.and_then(|bytes| ::serde_json::from_slice(&bytes).map_err(Into::into)))
        .map_err(|error| {
            anyhow!(
                "failed to set remote target ({name}: {origin}): {error}",
                name = &self.storage.name,
                origin = &self.storage.endpoint,
            )
        })
        .await
    }

    #[instrument(level = Level::INFO, skip(self, params, data), fields(data.len = data.as_ref().map(|data| data.len())), err(Display))]
    async fn execute<Param>(
        &self,
        method: Method,
        base_url: &str,
        params: &[(Param, Param)],
        data: Option<Bytes>,
    ) -> Result<Bytes>
    where
        Param: ToString,
    {
        let method = method.as_str().try_into().unwrap();

        let mut query_params = Multimap::default();
        for (key, value) in params {
            query_params.insert(key.to_string(), value.to_string());
        }

        self.storage
            .client
            .execute(
                method,
                &Default::default(),
                &mut Default::default(),
                &query_params,
                Some("minio"),
                Some(base_url),
                data,
            )
            .map_err(Into::into)
            .and_then(|response| response.bytes().map_err(Into::into))
            .await
    }

    fn encrypt_data<T>(
        &self,
        creds: Option<&Credentials>,
        data: &T,
    ) -> Result<Bytes, ::minio::s3::error::Error>
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

        let encrypted_data = {
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
        let mut ciphertext = BytesMut::default();
        ciphertext.extend(salt.to_vec());
        ciphertext.put_u8(ID);
        ciphertext.extend(nonce.to_vec());
        ciphertext.extend(encrypted_data);
        Ok(ciphertext.into())
    }
}

struct MinioTenantSpec<'a> {
    image: String,
    annotations: &'a BTreeMap<String, String>,
    labels: &'a BTreeMap<String, String>,
    owner_references: Option<&'a Vec<OwnerReference>>,
    pool_name: &'a str,
    prometheus_url: Option<&'a str>,
    storage: &'a ModelStorageObjectOwnedSpec,
}

#[instrument(level = Level::INFO, skip(kube), err(Display))]
async fn get_or_create_minio_tenant(
    kube: &Client,
    namespace: &str,
    name: &str,
    MinioTenantSpec {
        image,
        annotations,
        labels,
        owner_references,
        pool_name,
        prometheus_url,
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

    let labels_console = {
        let mut labels = labels.clone();
        labels.insert("v1.min.io/service".into(), "console".into());
        labels
    };
    let labels_minio = {
        let mut labels = labels.clone();
        labels.insert("v1.min.io/service".into(), "minio".into());
        labels
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
                annotations: Some(annotations.clone()),
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                owner_references: owner_references.cloned(),
                ..Default::default()
            },
            immutable: Some(false),
            string_data: Some(btreemap! {
                "config.env".into() => format!(
                    r#"
export MINIO_BROWSER="on"
export MINIO_STORAGE_CLASS_STANDARD="EC:{parity_level}"
export MINIO_ROOT_USER="{username}"
export MINIO_ROOT_PASSWORD="{password}"
"#,
                    username = random_string(16),
                    password = random_string(32),
                ),
            }),
            ..Default::default()
        })
        .await?
    };

    let secret_creds = {
        let name = format!("{name}-secret");
        get_or_create(&api_secret, "secret", &name, || Secret {
            metadata: ObjectMeta {
                annotations: Some(annotations.clone()),
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                owner_references: owner_references.cloned(),
                ..Default::default()
            },
            immutable: Some(true),
            string_data: Some(btreemap! {
                "accesskey".into() => Default::default(),
                "secretkey".into() => Default::default(),
            }),
            ..Default::default()
        })
        .await?
    };

    let secret_user_0 = {
        let name = format!("{name}-user-0");
        get_or_create(&api_secret, "secret", &name, || Secret {
            metadata: ObjectMeta {
                annotations: Some(annotations.clone()),
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                owner_references: owner_references.cloned(),
                ..Default::default()
            },
            immutable: Some(true),
            string_data: Some(btreemap! {
                "CONSOLE_ACCESS_KEY".into() => random_string(16),
                "CONSOLE_SECRET_KEY".into() => random_string(32),
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

    {
        let api = load_api_tenant(kube, namespace).await?;
        let data = || {
            Ok(DynamicObject {
                types: Some(TypeMeta {
                    api_version: "minio.min.io/v2".into(),
                    kind: "Tenant".into(),
                }),
                metadata: ObjectMeta {
                    annotations: Some({
                        let mut annotations = annotations.clone();
                        annotations.insert(
                            "prometheus.io/path".into(),
                            "/minio/v2/metrics/cluster".into(),
                        );
                        annotations.insert("prometheus.io/port".into(), "9000".into());
                        annotations.insert("prometheus.io/scrape".into(), "true".into());
                        annotations
                    }),
                    labels: Some(labels.clone()),
                    name: Some(name.to_string()),
                    namespace: Some(namespace.to_string()),
                    owner_references: owner_references.cloned(),
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
                        "env": [
                            {
                                "name": "MINIO_PROMETHEUS_AUTH_TYPE",
                                "value": "public",
                            },
                            {
                                "name": "MINIO_PROMETHEUS_JOB_ID",
                                "value": format!("{name}-minio-job"),
                            },
                            {
                                "name": "MINIO_PROMETHEUS_URL",
                                "value": prometheus_url.ok_or_else(|| anyhow!("no such storage: {namespace}/{name}"))?,
                            },
                        ],
                        "exposeServices": {
                            "console": minio_console_external_service.is_enabled(),
                            "minio": minio_external_service.is_enabled(),
                        },
                        "image": image,
                        "imagePullPolicy": "Always",
                        "imagePullSecret": {},
                        "mountPath": "/export",
                        "pools": [
                            {
                                "affinity": {
                                    "nodeAffinity": get_default_node_affinity(),
                                    "podAffinity": get_default_pod_affinity(name),
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
                                // "annotations": annotations,
                                "labels": labels,
                                "name": pool_name,
                                "resources": compute_resources,
                                "runtimeClassName": runtime_class_name,
                                "servers": total_nodes,
                                "volumeClaimTemplate": {
                                    "metadata": {
                                        "annotations": annotations,
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
                            "consoleServiceLabels": labels_console,
                            "minioServiceAnnotations": {
                                "metallb.universe.tf/address-pool": minio_external_service.address_pool,
                                "metallb.universe.tf/loadBalancerIPs": minio_external_service.ip,
                            },
                            "minioServiceLabels": labels_minio,
                        },
                        "users": users,
                    },
                }),
            })
        };
        get_or_try_create(&api, "tenant", name, data).await?
    };
    {
        let api = load_api_service_monitor(kube, namespace).await?;
        let name = format!("{name}-minio");
        let data = || DynamicObject {
            types: Some(TypeMeta {
                api_version: "monitoring.coreos.com/v1".into(),
                kind: "ServiceMonitor".into(),
            }),
            metadata: ObjectMeta {
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                namespace: Some(namespace.to_string()),
                owner_references: owner_references.cloned(),
                ..Default::default()
            },
            data: json!({
                "spec": {
                    "endpoints": [
                        {
                            "path": "/minio/v2/metrics/bucket",
                            "port": "http-minio",
                            "interval": "5s",
                        },
                        {
                            "path": "/minio/v2/metrics/cluster",
                            "port": "http-minio",
                            "interval": "5s",
                        },
                        {
                            "path": "/minio/v2/metrics/resource",
                            "port": "http-minio",
                            "interval": "5s",
                        },
                    ],
                    "namespaceSelector": {
                        "matchNames": [
                            namespace,
                        ],
                    },
                    "selector": {
                        "matchLabels": labels,
                    },
                },
            }),
        };
        get_or_create(&api, "servicemonitor", &name, data).await?
    };

    Ok(secret_user_0)
}

#[derive(Default)]
struct BucketJobSpec<'a> {
    delete_source: bool,
    delete_target: bool,
    source: Option<&'a ObjectStorageSession>,
    sync_source: bool,
    sync_source_overwrite: bool,
}

async fn get_or_create<K, Data>(api: &Api<K>, kind: &str, name: &str, data: Data) -> Result<K>
where
    Data: FnOnce() -> K,
    K: Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    get_or_try_create(api, kind, name, || Ok(data())).await
}

#[instrument(level = Level::INFO, skip(api, data), err(Display))]
async fn get_or_try_create<K, Data>(api: &Api<K>, kind: &str, name: &str, data: Data) -> Result<K>
where
    Data: FnOnce() -> Result<K>,
    K: Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    match api.get_opt(name).await {
        Ok(Some(value)) => Ok(value),
        Ok(None) => {
            let pp = PostParams {
                dry_run: false,
                field_manager: Some(crate::NAME.into()),
            };
            api.create(&pp, &data()?)
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
                storage_resource_as_bytes.as_u128() / total_volumes as u128;
            Ok(Some(btreemap! {
                "storage".into() => Quantity(storage_resource_per_volume.to_string()),
            }))
        } else {
            Ok(Some(Default::default()))
        }
    }

    let mut compute = resources.clone();
    let storage = ResourceRequirements {
        claims: compute.claims.clone(),
        limits: split_storage(&mut compute.limits, total_volumes, true)?,
        requests: split_storage(&mut compute.requests, total_volumes, false)?,
    };
    Ok((
        ModelStorageObjectOwnedReplicationComputeResource(compute),
        ModelStorageObjectOwnedReplicationStorageResource(storage),
    ))
}

fn get_default_node_affinity() -> NodeAffinity {
    NodeAffinity {
        preferred_during_scheduling_ignored_during_execution: Some(vec![
            // KISS normal control plane nodes should be preferred
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
            // KISS normal control plane nodes should be preferred
            PreferredSchedulingTerm {
                preference: NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec!["ControlPlane".into()]),
                    }]),
                    match_fields: None,
                },
                weight: 2,
            },
            // KISS compute nodes should be preferred
            PreferredSchedulingTerm {
                preference: NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec!["Compute".into()]),
                    }]),
                    match_fields: None,
                },
                weight: 4,
            },
            // KISS gateway nodes should be more preferred
            PreferredSchedulingTerm {
                preference: NodeSelectorTerm {
                    match_expressions: Some(vec![NodeSelectorRequirement {
                        key: "node-role.kubernetes.io/kiss".into(),
                        operator: "In".into(),
                        values: Some(vec!["Gateway".into()]),
                    }]),
                    match_fields: None,
                },
                weight: 8,
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
                        "Desktop".into(),
                        "Gateway".into(),
                    ]),
                }]),
                match_fields: None,
            }],
        }),
    }
}

fn get_default_pod_affinity(tenant_name: &str) -> PodAffinity {
    PodAffinity {
        preferred_during_scheduling_ignored_during_execution: Some(vec![WeightedPodAffinityTerm {
            pod_affinity_term: PodAffinityTerm {
                label_selector: Some(LabelSelector {
                    match_expressions: Some(vec![LabelSelectorRequirement {
                        key: "dash.ulagbulag.io/modelstorage-type".into(),
                        operator: "In".into(),
                        values: Some(vec![tenant_name.into()]),
                    }]),
                    match_labels: None,
                }),
                topology_key: "kubernetes.io/hostname".into(),
                ..Default::default()
            },
            weight: 32,
        }]),
        required_during_scheduling_ignored_during_execution: None,
    }
}

const fn get_default_tenant_name() -> &'static str {
    "object-storage"
}

fn get_default_ingress_annotations() -> BTreeMap<String, String> {
    btreemap! {
        // max single payload size; it can be virtually increased by using multi-part uploading
        "nginx.ingress.kubernetes.io/proxy-body-size".into() => "100M".into(),
        "nginx.ingress.kubernetes.io/proxy-read-timeout".into() => "3600".into(),
        "nginx.ingress.kubernetes.io/proxy-send-timeout".into() => "3600".into(),
    }
}

fn get_ingress_class_name(namespace: &str, tenant_name: &str) -> String {
    format!("dash.{tenant_name}.{namespace}")
}

#[instrument(level = Level::INFO, skip(kube, namespace), err(Display))]
async fn load_api_service_monitor(kube: &Client, namespace: &str) -> Result<Api<DynamicObject>> {
    let client = super::kubernetes::KubernetesStorageClient { namespace, kube };
    let spec = ModelCustomResourceDefinitionRefSpec {
        name: "servicemonitors.monitoring.coreos.com/v1".into(),
    };
    client
        .api_custom_resource(&spec, None)
        .await
        .map_err(Into::into)
}

#[instrument(level = Level::INFO, skip(kube, namespace), err(Display))]
async fn load_api_tenant(kube: &Client, namespace: &str) -> Result<Api<DynamicObject>> {
    let client = super::kubernetes::KubernetesStorageClient { namespace, kube };
    let spec = ModelCustomResourceDefinitionRefSpec {
        name: "tenants.minio.min.io/v2".into(),
    };
    client
        .api_custom_resource(&spec, None)
        .await
        .map_err(Into::into)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteTarget {
    arn: String,
    credentials: RemoteTargetCredentials,
    endpoint: String,
    sourcebucket: String,
    targetbucket: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteTargetCredentials {
    access_key: String,
}

struct ModelStorageObjectOwnedReplicationComputeResource(ResourceRequirements);

struct ModelStorageObjectOwnedReplicationStorageResource(ResourceRequirements);
