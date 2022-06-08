use ipis::{core::anyhow::Result, log::info};
use k8s_openapi::api::{
    batch::v1::{Job, JobSpec},
    core::v1::{
        ConfigMapKeySelector, ConfigMapVolumeSource, Container, EnvVar, EnvVarSource, KeyToPath,
        PodSpec, PodTemplateSpec, SecretVolumeSource, Volume, VolumeMount,
    },
};
use kube::{api::PostParams, core::ObjectMeta, Api, Client, Error};

use crate::r#box::{BoxAccessSpec, BoxMachineSpec, BoxState};

#[derive(Default)]
pub struct AnsibleClient {}

impl AnsibleClient {
    pub const ANNOTATION_COMPLETED_STATE: &'static str = "kiss.netai-cloud/completed_state";

    pub async fn spawn(&self, kube: &Client, job: AnsibleJob<'_>) -> Result<(), Error> {
        let ns = "kiss";
        let name = format!("box-{}-{}", &job.task, &job.machine.uuid);

        let api = Api::<Job>::namespaced(kube.clone(), ns);
        if api.get(&name).await.is_ok() {
            info!("job is already running: {name}");
            return Ok(());
        }

        let job = Job {
            metadata: ObjectMeta {
                name: Some(name.clone()),
                namespace: Some(ns.into()),
                annotations: job
                    .completed_state
                    .as_ref()
                    .map(ToString::to_string)
                    .map(|state| {
                        vec![(Self::ANNOTATION_COMPLETED_STATE.into(), state)]
                            .into_iter()
                            .collect()
                    }),
                ..Default::default()
            },
            spec: Some(JobSpec {
                ttl_seconds_after_finished: None,
                template: PodTemplateSpec {
                    spec: Some(PodSpec {
                        restart_policy: Some("Never".into()),
                        service_account: Some("ansible-playbook".into()),
                        containers: vec![Container {
                            name: "ansible".into(),
                            image: Some("cytopia/ansible:latest-awsk8s".into()),
                            command: Some(vec!["ansible-playbook".into()]),
                            args: Some(vec!["-vvv".into(), "/opt/playbook/playbook.yaml".into()]),
                            env: Some(vec![
                                EnvVar {
                                    name: "ansible_host".into(),
                                    value: Some(format!("{}.box.kiss-cluster", &job.machine.uuid)),
                                    ..Default::default()
                                },
                                EnvVar {
                                    name: "ansible_host_id".into(),
                                    value: Some(job.machine.uuid.to_string()),
                                    ..Default::default()
                                },
                                EnvVar {
                                    name: "ansible_ssh_host".into(),
                                    value: Some(job.access.address.to_string()),
                                    ..Default::default()
                                },
                                EnvVar {
                                    name: "ansible_user".into(),
                                    value_from: Some(EnvVarSource {
                                        config_map_key_ref: Some(ConfigMapKeySelector {
                                            name: Some("matchbox-account".into()),
                                            key: "username".into(),
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                },
                                EnvVar {
                                    name: "ansible_ssh_private_key_file".into(),
                                    value: Some("/root/.ssh/id_rsa".into()),
                                    ..Default::default()
                                },
                                EnvVar {
                                    name: "ansible_ipmi_password".into(),
                                    value: Some("kiss".into()),
                                    ..Default::default()
                                },
                            ]),
                            volume_mounts: Some(vec![
                                VolumeMount {
                                    name: "playbook".into(),
                                    mount_path: "/opt/playbook".into(),
                                    ..Default::default()
                                },
                                VolumeMount {
                                    name: "tasks".into(),
                                    mount_path: "/opt/playbook/tasks".into(),
                                    ..Default::default()
                                },
                                VolumeMount {
                                    name: "ssh".into(),
                                    mount_path: "/root/.ssh".into(),
                                    ..Default::default()
                                },
                            ]),
                            ..Default::default()
                        }],
                        volumes: Some(vec![
                            Volume {
                                name: "playbook".into(),
                                config_map: Some(ConfigMapVolumeSource {
                                    name: Some("ansible-playbook".into()),
                                    default_mode: Some(0o400),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            Volume {
                                name: "tasks".into(),
                                config_map: Some(ConfigMapVolumeSource {
                                    name: Some(format!("ansible-task-{}", &job.task)),
                                    default_mode: Some(0o400),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            Volume {
                                name: "ssh".into(),
                                secret: Some(SecretVolumeSource {
                                    secret_name: Some("matchbox-account".into()),
                                    default_mode: Some(0o400),
                                    items: Some(vec![KeyToPath {
                                        key: "id_rsa".into(),
                                        path: "id_rsa".into(),
                                        ..Default::default()
                                    }]),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }),
            status: None,
        };
        let pp = PostParams {
            dry_run: false,
            field_manager: Some("kube-controller".into()),
        };
        api.create(&pp, &job).await?;

        info!("spawned a job: {name}");
        Ok(())
    }
}

pub struct AnsibleJob<'a> {
    pub task: &'a str,
    pub access: &'a BoxAccessSpec,
    pub machine: &'a BoxMachineSpec,
    pub completed_state: Option<BoxState>,
}
