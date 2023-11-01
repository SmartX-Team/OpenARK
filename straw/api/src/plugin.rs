use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use ark_core_k8s::data::Url;
use async_trait::async_trait;
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        core::v1::{
            Affinity, Container, EnvVar, KeyToPath, NodeAffinity, NodeSelector,
            NodeSelectorRequirement, NodeSelectorTerm, PodAffinityTerm, PodAntiAffinity, PodSpec,
            PodTemplateSpec, PreferredSchedulingTerm, ProjectedVolumeSource, SecretProjection,
            Volume, VolumeMount, VolumeProjection, WeightedPodAffinityTerm,
        },
    },
    apimachinery::pkg::apis::meta::v1::LabelSelector,
};
use kube::{
    api::{DeleteParams, PostParams},
    core::ObjectMeta,
    Api, Client,
};

use crate::pipe::StrawNode;

pub trait PluginBuilder {
    fn try_build(&self, url: &Url) -> Option<DynPlugin>;
}

pub type DynPlugin = Box<dyn Send + Plugin>;

#[async_trait]
pub trait Plugin
where
    Self: Send + Sync,
{
    async fn create(&self, client: Client, namespace: Option<&str>, node: &StrawNode)
        -> Result<()>;

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
    fn container_default_env(&self) -> Vec<EnvVar> {
        vec![]
    }

    fn container_image(&self) -> String {
        "quay.io/ulagbulag/openark:latest".into()
    }

    fn container_command(&self) -> Option<Vec<String>> {
        None
    }

    fn container_command_args(&self) -> Option<Vec<String>> {
        None
    }
}

#[async_trait]
impl<T> Plugin for T
where
    Self: Send + Sync + PluginDaemon,
{
    async fn create(
        &self,
        client: Client,
        namespace: Option<&str>,
        node: &StrawNode,
    ) -> Result<()> {
        let api = match namespace {
            Some(namespace) => Api::<Deployment>::namespaced(client, namespace),
            None => Api::<Deployment>::default_namespaced(client),
        };
        let pp = PostParams {
            dry_run: false,
            field_manager: Some(crate::name::NAME.into()),
        };
        api.create(&pp, &self.build_job(node))
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to create straw deployment on k8s: {error}"))
    }

    async fn delete(
        &self,
        client: Client,
        namespace: Option<&str>,
        node: &StrawNode,
    ) -> Result<()> {
        let api = match namespace {
            Some(namespace) => Api::<Deployment>::namespaced(client, namespace),
            None => Api::<Deployment>::default_namespaced(client),
        };
        let dp = DeleteParams::default();
        api.delete(&node.name, &dp)
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to delete straw deployment on k8s: {error}"))
    }

    async fn exists(
        &self,
        client: Client,
        namespace: Option<&str>,
        node: &StrawNode,
    ) -> Result<bool> {
        let api = match namespace {
            Some(namespace) => Api::<Deployment>::namespaced(client, namespace),
            None => Api::<Deployment>::default_namespaced(client),
        };
        api.get_metadata_opt(&node.name)
            .await
            .map(|option| option.is_some())
            .map_err(|error| anyhow!("failed to check straw deployment on k8s: {error}"))
    }
}

trait PluginDaemonExt {
    fn build_job(&self, node: &StrawNode) -> Deployment;
}

impl<T> PluginDaemonExt for T
where
    Self: PluginDaemon,
{
    fn build_job(&self, node: &StrawNode) -> Deployment {
        // TODO: to be implemented! (PIPE_MODEL_IN, PIPE_MODEL_OUT, ... from **PIPE!!!** )
        let mut env = self.container_default_env();
        for var in node.env.iter().cloned() {
            match env.iter_mut().find(|stored| stored.name == var.name) {
                Some(stored) => *stored = var,
                None => env.push(var),
            }
        }

        // TODO: to be implemented!
        let labels = BTreeMap::default();

        // TODO: to be implemented!
        let service_account = "nats-admin-user".to_string();

        let nats_volume_name = "nats-token";

        Deployment {
            metadata: ObjectMeta {
                labels: Some(labels.clone()),
                name: Some(node.name.clone()),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                selector: LabelSelector {
                    match_expressions: None,
                    match_labels: Some(labels.clone()),
                },
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
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
                                            namespace_selector: None,
                                            namespaces: None,
                                            topology_key: "kubernetes.io/hostname".into(),
                                        },
                                        weight: 1,
                                    },
                                ]),
                                required_during_scheduling_ignored_during_execution: None,
                            }),
                        }),
                        containers: vec![
                            Container {
                                args: self.container_command_args(),
                                command: self.container_command(),
                                env: Some(env),
                                image: Some(self.container_image()),
                                image_pull_policy: Some("Always".into()),
                                name: "function".into(),
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
                        service_account: Some(service_account.clone()),
                        volumes: Some(vec![
                            Volume {
                                name: nats_volume_name.into(),
                                projected: Some(ProjectedVolumeSource {
                                    default_mode: None,
                                    sources: Some(vec![
                                        VolumeProjection {
                                            secret: Some(SecretProjection {
                                                name: Some(format!("{service_account}-nats-bound-token")),
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
