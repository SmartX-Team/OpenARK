use std::fmt;

use anyhow::{anyhow, Result};
use ark_core_k8s::data::{Name, Url};
use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        core::v1::{
            Affinity, Container, EnvVar, EnvVarSource, KeyToPath, NodeAffinity, NodeSelector,
            NodeSelectorRequirement, NodeSelectorTerm, PodAffinityTerm, PodAntiAffinity, PodSpec,
            PodTemplateSpec, PreferredSchedulingTerm, ProjectedVolumeSource, ResourceRequirements,
            SecretKeySelector, SecretProjection, Volume, VolumeMount, VolumeProjection,
            WeightedPodAffinityTerm,
        },
    },
    apimachinery::pkg::apis::meta::v1::LabelSelector,
    DeepMerge, NamespaceResourceScope,
};
use kube::{
    api::{DeleteParams, PostParams},
    core::{ObjectMeta, PartialObjectMeta},
    Api, Client, Resource,
};
use maplit::btreemap;
use serde::de::DeserializeOwned;
use tracing::{instrument, Level};

use crate::function::{StrawFunctionType, StrawNode};

pub trait PluginBuilder {
    fn try_build(&self, url: &Url) -> Option<DynPlugin>;
}

pub type DynPlugin = Box<dyn Send + Plugin>;

#[async_trait]
pub trait Plugin
where
    Self: Send + Sync,
{
    async fn create(
        &self,
        client: Client,
        namespace: Option<&str>,
        ctx: &PluginContext,
        node: &StrawNode,
    ) -> Result<()>;

    async fn delete(&self, client: Client, namespace: Option<&str>, node: &StrawNode)
        -> Result<()>;

    async fn exists(
        &self,
        client: Client,
        namespace: Option<&str>,
        node: &StrawNode,
    ) -> Result<bool>;
}

pub trait PluginDaemon {
    fn container_default_env(&self, _node: &StrawNode) -> Vec<EnvVar> {
        vec![]
    }

    fn container_image(&self) -> String {
        "quay.io/ulagbulag/openark:latest-full".into()
    }

    fn container_command(&self, _env: &[EnvVar]) -> Option<Vec<String>> {
        None
    }

    fn container_command_args(&self, _env: &[EnvVar]) -> Option<Vec<String>> {
        None
    }

    fn container_resources(&self) -> Option<ResourceRequirements> {
        None
    }
}

#[async_trait]
impl<T> Plugin for T
where
    Self: Send + Sync + PluginDaemon,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn create(
        &self,
        client: Client,
        namespace: Option<&str>,
        ctx: &PluginContext,
        node: &StrawNode,
    ) -> Result<()> {
        let api: Api<Deployment> = load_api(client, namespace);
        let mut data = self.build_job(ctx, node);

        match exists(&api, node).await? {
            Some(metadata) => {
                data.metadata.merge_from(metadata.metadata);

                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some(crate::name::NAME.into()),
                };
                api.replace(&node.name, &pp, &data)
                    .await
                    .map(|_| ())
                    .map_err(|error| anyhow!("failed to create straw deployment on k8s: {error}"))
            }
            None => {
                let pp = PostParams {
                    dry_run: false,
                    field_manager: Some(crate::name::NAME.into()),
                };
                api.create(&pp, &data)
                    .await
                    .map(|_| ())
                    .map_err(|error| anyhow!("failed to update straw deployment on k8s: {error}"))
            }
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn delete(
        &self,
        client: Client,
        namespace: Option<&str>,
        node: &StrawNode,
    ) -> Result<()> {
        let api: Api<Deployment> = load_api(client, namespace);

        if exists(&api, node).await?.is_some() {
            let dp = DeleteParams::default();
            api.delete(&node.name, &dp)
                .await
                .map(|_| ())
                .map_err(|error| anyhow!("failed to delete straw deployment on k8s: {error}"))
        } else {
            Ok(())
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn exists(
        &self,
        client: Client,
        namespace: Option<&str>,
        node: &StrawNode,
    ) -> Result<bool> {
        let api: Api<Deployment> = load_api(client, namespace);
        exists(&api, node).await.map(|option| option.is_some())
    }
}

#[instrument(level = Level::INFO, skip_all, err(Display))]
async fn exists<K>(api: &Api<K>, node: &StrawNode) -> Result<Option<PartialObjectMeta<K>>>
where
    K: Clone + fmt::Debug + DeserializeOwned,
{
    api.get_metadata_opt(&node.name)
        .await
        .map_err(|error| anyhow!("failed to check straw deployment on k8s: {error}"))
}

fn load_api<K>(client: Client, namespace: Option<&str>) -> Api<K>
where
    K: Clone + fmt::Debug + DeserializeOwned + Resource<Scope = NamespaceResourceScope>,
    <K as Resource>::DynamicType: Default,
{
    match namespace {
        Some(namespace) => Api::namespaced(client, namespace),
        None => Api::default_namespaced(client),
    }
}

trait PluginDaemonExt {
    fn build_job(&self, ctx: &PluginContext, node: &StrawNode) -> Deployment;
}

impl<T> PluginDaemonExt for T
where
    Self: PluginDaemon,
{
    fn build_job(&self, ctx: &PluginContext, node: &StrawNode) -> Deployment {
        let name = &node.name;
        let service_account = ctx.service_account.clone();

        // load default env
        let mut env = self.container_default_env(node);
        ctx.apply_to_env(&mut env);

        // load user-defined env
        for var in node.env.iter().cloned() {
            match env.iter_mut().find(|stored| stored.name == var.name) {
                Some(stored) => *stored = var,
                None => env.push(var),
            }
        }

        // infer required missing env
        try_apply_to_env(&mut env, "NATS_ACCOUNT", &service_account);
        try_apply_to_env(&mut env, "PIPE_MODEL_IN", format!("{name}.in"));
        try_apply_to_env(&mut env, "PIPE_MODEL_OUT", format!("{name}.out"));

        let env = env
            .into_iter()
            .filter(|env| {
                env.value
                    .as_ref()
                    .map(|value| !value.is_empty())
                    .unwrap_or_default()
                    || env.value_from.is_some()
            })
            .collect();

        let annotations = btreemap! {
            "dash.ulagbulag.io/timestamp".into() => Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            "instrumentation.opentelemetry.io/inject-sdk".into() => true.to_string(),
        };
        let labels = btreemap! {
            "dash.ulagbulag.io/plugin".into() => name.clone(),
            "dash.ulagbulag.io/service-account".into() => service_account.clone(),
        };

        let nats_token_secret_name = format!("{service_account}-nats-bound-token");
        let nats_volume_name = "nats-token";

        Deployment {
            metadata: ObjectMeta {
                annotations: Some(annotations.clone()),
                labels: Some(labels.clone()),
                name: Some(name.clone()),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                selector: LabelSelector {
                    match_expressions: None,
                    match_labels: Some(labels.clone()),
                },
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        annotations: Some(annotations),
                        labels: Some(labels.clone()),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        affinity: Some(Affinity {
                            node_affinity: Some(NodeAffinity {
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
                                                values: Some(vec!["GenericWorker".into()]),
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
                                                values: Some(vec!["Compute".into()]),
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
                                                "GenericWorker".into(),
                                            ]),
                                        }]),
                                        match_fields: None,
                                    }],
                                }),
                            }),
                            pod_affinity: None,
                            pod_anti_affinity: Some(PodAntiAffinity {
                                preferred_during_scheduling_ignored_during_execution: Some(vec![
                                    WeightedPodAffinityTerm {
                                        pod_affinity_term: PodAffinityTerm {
                                            label_selector: Some(LabelSelector {
                                                match_expressions: None,
                                                match_labels: Some(labels),
                                            }),
                                            topology_key: "kubernetes.io/hostname".into(),
                                            ..Default::default()
                                        },
                                        weight: 1,
                                    },
                                ]),
                                required_during_scheduling_ignored_during_execution: None,
                            }),
                        }),
                        containers: vec![
                            Container {
                                args: self.container_command_args(&node.env),
                                command: self.container_command(&node.env),
                                env: Some(env),
                                image: Some(self.container_image()),
                                image_pull_policy: Some("Always".into()),
                                name: "function".into(),
                                resources: {
                                    let mut resources = self.container_resources();
                                    resources.merge_from(node.resources.clone());
                                    resources
                                },
                                volume_mounts: Some(vec![
                                    VolumeMount {
                                        name: nats_volume_name.into(),
                                        // NOTE: default path
                                        mount_path: "/var/run/secrets/nats.io".into(),
                                        ..Default::default()
                                    },
                                ]),
                                ..Default::default()
                            },
                        ],
                        service_account: Some(service_account),
                        volumes: Some(vec![
                            Volume {
                                name: nats_volume_name.into(),
                                projected: Some(ProjectedVolumeSource {
                                    default_mode: None,
                                    sources: Some(vec![
                                        VolumeProjection {
                                            secret: Some(SecretProjection {
                                                name: Some(nats_token_secret_name),
                                                items: Some(vec![
                                                    KeyToPath {
                                                        key: "token".into(),
                                                        mode: Some(0o444),
                                                        path: "token".into(),
                                                    },
                                                ]),
                                                optional: Some(false),
                                            }),
                                            ..Default::default()
                                        },
                                    ]),
                                }),
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            status: None,
        }
    }
}

macro_rules! context_env_fields {
    (
        env: {
            $(
                #[
                    env ( $env_env:expr ) ,
                    default ( $env_default:expr ) ,
                ]
                $env_vis:vis $env_name:ident : $env_ty:ty ,
            )*
            optional: {
                $(
                    #[
                        env ( $env_optional_env:expr ) ,
                        default ( $env_optional_default:expr ) ,
                    ]
                    $env_optional_vis:vis $env_optional_name:ident : Option < $env_optional_ty:ty > ,
                )*
            },
        },
        env_from_secret: { $(
            #[
                env ( $env_from_secret_env:expr ) ,
                secret (
                    key = $env_from_secret_secret_key:expr ,
                    name = $env_from_secret_secret_name:expr ,
                ) ,
            ]
            $env_from_secret_vis:vis $env_from_secret_name:ident : Option < $env_from_secret_ty:ty > ,
        )* },
        ext: {
            $(
                #[
                    default ( $ext_default:expr )
                ]
                $ext_vis:vis $ext_name:ident : $ext_ty:ty ,
            )*
            optional: {
                $(
                    #[
                        default ( $ext_optional_default:expr )
                    ]
                    $ext_optional_vis:vis $ext_optional_name:ident : Option < $ext_optional_ty:ty > ,
                )*
            },
        },
    ) => {
        #[derive(Clone, Debug, PartialEq)]
        #[cfg_attr(feature = "clap", derive(::clap::Parser))]
        pub struct PluginContext {
            $(
                #[
                    cfg_attr(
                        feature = "clap",
                        arg(
                            long,
                            env = $env_env,
                            default_value_t = $env_default,
                        )
                    )
                ]
                $env_vis $env_name : $env_ty ,
            )*
            $(
                #[
                    cfg_attr(
                        feature = "clap",
                        arg(
                            long,
                            env = $env_optional_env,
                        )
                    )
                ]
                $env_optional_vis $env_optional_name : Option < $env_optional_ty > ,
            )*
            $(
                #[
                    cfg_attr(
                        feature = "clap",
                        arg(
                            long,
                            env = $env_from_secret_env,
                        )
                    )
                ]
                $env_from_secret_vis $env_from_secret_name : Option < $env_from_secret_ty > ,
            )*
            $(
                #[
                    cfg_attr(
                        feature = "clap",
                        arg(
                            long,
                            default_value_t = $ext_default,
                        )
                    )
                ]
                $ext_vis $ext_name : $ext_ty ,
            )*
            $(
                #[
                    cfg_attr(
                        feature = "clap",
                        arg(
                            long,
                        )
                    )
                ]
                $ext_optional_vis $ext_optional_name : Option < $ext_optional_ty > ,
            )*
        }

        impl Default for PluginContext {
            fn default() -> Self {
                Self {
                    $(
                        $env_name: $env_default,
                    )*
                    $(
                        $env_optional_name: $env_optional_default,
                    )*
                    $(
                        $env_from_secret_name: None,
                    )*
                    $(
                        $ext_name: $ext_default,
                    )*
                    $(
                        $ext_optional_name: $ext_optional_default,
                    )*
                }
            }
        }

        impl PluginContext {
            fn apply_to_env(&self, env: &mut Vec<EnvVar>) {
                $(
                    apply_to_env(
                        env,
                        $env_env,
                        &self.$env_name,
                    );
                )*
                $(
                    apply_to_env(
                        env,
                        $env_optional_env,
                        &self.$env_optional_name,
                    );
                )*
                $(
                    match &self.$env_from_secret_name {
                        Some(value) => apply_to_env(
                            env,
                            $env_from_secret_env,
                            value,
                        ),
                        None => apply_to_env_from(
                            env,
                            $env_from_secret_env,
                            EnvVarSource {
                                secret_key_ref: Some(SecretKeySelector {
                                    key: $env_from_secret_secret_key.into(),
                                    name: Some($env_from_secret_secret_name.into()),
                                    optional: Some(false),
                                }),
                                ..Default::default()
                            },
                        ),
                    }
                )*
            }
        }
    };
}

context_env_fields! {
    env: {
        #[
            env("AWS_ENDPOINT_URL"),
            default(String::from("http://minio")),
        ]
        pub aws_endpoint_url: String,

        #[
            env("KAFKA_HOSTS"),
            default(String::from("kafka-kafka-bootstrap")),
        ]
        pub kafka_hosts: String,

        #[
            env("RUST_LOG"),
            default(Level::INFO),
        ]
        pub log_level: Level,

        #[
            env("PIPE_DEFAULT_MESSENGER"),
            default(String::from("Nats")),
        ]
        pub messenger: String,

        #[
            env("NATS_ADDRS"),
            default(String::from("nats")),
        ]
        pub nats_addrs: String,

        #[
            env("NATS_PASSWORD_PATH"),
            default(String::from("/var/run/secrets/nats.io/token")),
        ]
        pub nats_password_path: String,

        #[
            env("PIPE_PERSISTENCE"),
            default(false),
        ]
        pub persistent: bool,

        #[
            env("PIPE_PERSISTENCE_METADATA"),
            default(false),
        ]
        pub persistent_metadata: bool,

        #[
            env("PIPE_QUEUE_GROUP"),
            default(false),
        ]
        pub queue_group: bool,

        optional: {
            #[
                env("PIPE_ENCODER"),
                default(None),
            ]
            pub encoder: Option<String>,

            #[
                env("PIPE_MAX_TASKS"),
                default(None),
            ]
            pub max_tasks: Option<u32>,

            #[
                env("PIPE_MODEL_IN"),
                default(None),
            ]
            pub model_in: Option<Name>,

            #[
                env("PIPE_MODEL_OUT"),
                default(None),
            ]
            pub model_out: Option<Name>,

            #[
                env("NATS_ACCOUNT"),
                default(None),
            ]
            pub nats_account: Option<String>,
        },
    },
    env_from_secret: {
        #[
            env("AWS_ACCESS_KEY_ID"),
            secret(
                key = "CONSOLE_ACCESS_KEY",
                name = "object-storage-user-0",
            ),
        ]
        pub aws_access_key_id: Option<String>,

        #[
            env("AWS_SECRET_ACCESS_KEY"),
            secret(
                key = "CONSOLE_SECRET_KEY",
                name = "object-storage-user-0",
            ),
        ]
        pub aws_secret_access_key: Option<String>,
    },
    ext: {
        #[default(String::from("nats-admin"))]
        pub service_account: String,

        optional: {},
    },
}

impl PluginContext {
    pub fn new(type_: StrawFunctionType, model_in: Option<Name>, model_out: Option<Name>) -> Self {
        match type_ {
            StrawFunctionType::OneShot => Self::default(),
            StrawFunctionType::Pipe => Self {
                model_in,
                model_out,
                ..Default::default()
            },
        }
    }
}

trait EnvValue {
    fn to_string_value(&self) -> Option<String>;
}

impl EnvValue for bool {
    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl EnvValue for u32 {
    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl EnvValue for String {
    fn to_string_value(&self) -> Option<String> {
        Some(self.clone())
    }
}

impl EnvValue for Name {
    fn to_string_value(&self) -> Option<String> {
        Some(self.clone().into())
    }
}

impl EnvValue for Level {
    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl<T> EnvValue for &T
where
    T: EnvValue,
{
    fn to_string_value(&self) -> Option<String> {
        <T as EnvValue>::to_string_value(*self)
    }
}

impl<T> EnvValue for Option<T>
where
    T: EnvValue,
{
    fn to_string_value(&self) -> Option<String> {
        self.as_ref().and_then(|value| value.to_string_value())
    }
}

fn try_apply_to_env(env: &mut Vec<EnvVar>, name: &str, value: impl EnvValue) {
    let var = EnvVar {
        name: name.into(),
        value: value.to_string_value(),
        value_from: None,
    };

    match env.iter_mut().find(|stored| stored.name == var.name) {
        Some(stored) => {
            if stored
                .value
                .as_ref()
                .map(|value| value.is_empty())
                .unwrap_or(true)
            {
                *stored = var;
            }
        }
        None => env.push(var),
    }
}

fn apply_to_env(env: &mut Vec<EnvVar>, name: &str, value: impl EnvValue) {
    let var = EnvVar {
        name: name.into(),
        value: value.to_string_value(),
        value_from: None,
    };

    match env.iter_mut().find(|stored| stored.name == var.name) {
        Some(stored) => *stored = var,
        None => env.push(var),
    }
}

fn apply_to_env_from(env: &mut Vec<EnvVar>, name: &str, value: EnvVarSource) {
    let var = EnvVar {
        name: name.into(),
        value: None,
        value_from: Some(value),
    };

    match env.iter_mut().find(|stored| stored.name == name) {
        Some(stored) => *stored = var,
        None => env.push(var),
    }
}
