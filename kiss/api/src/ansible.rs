use inflector::Inflector;
use ipis::{core::anyhow::Result, log::info};
use k8s_openapi::{
    api::{
        batch::v1::{CronJob, CronJobSpec, Job, JobSpec, JobTemplateSpec},
        core::v1::{
            ConfigMapKeySelector, ConfigMapVolumeSource, Container, EnvVar, EnvVarSource,
            KeyToPath, PodSpec, PodTemplateSpec, ResourceRequirements, SecretKeySelector,
            SecretVolumeSource, Volume, VolumeMount,
        },
    },
    apimachinery::pkg::api::resource::Quantity,
};
use kube::{
    api::{DeleteParams, ListParams, PostParams},
    core::ObjectMeta,
    Api, Client, Error,
};

use crate::{
    cluster::ClusterState,
    config::KissConfig,
    r#box::{BoxCrd, BoxPowerSpec, BoxState},
};

pub struct AnsibleClient {
    pub kiss: KissConfig,
}

impl AnsibleClient {
    pub const LABEL_BOX_NAME: &'static str = "kiss.netai-cloud/box_name";
    pub const LABEL_BOX_ACCESS_PRIMARY_ADDRESS: &'static str =
        "kiss.netai-cloud/box_access_primary_address";
    pub const LABEL_BOX_ACCESS_PRIMART_SPEED_MBPS: &'static str =
        "kiss.netai-cloud/box_access_primary_speed_mbps";
    pub const LABEL_BOX_MACHINE_UUID: &'static str = "kiss.netai-cloud/box_machine_uuid";
    pub const LABEL_COMPLETED_STATE: &'static str = "kiss.netai-cloud/completed_state";
    pub const LABEL_GROUP_CLUSTER_NAME: &'static str = "kiss.netai-cloud/group_cluster_name";
    pub const LABEL_GROUP_ROLE: &'static str = "kiss.netai-cloud/group_role";

    pub async fn try_default(kube: &Client) -> Result<Self> {
        Ok(Self {
            kiss: KissConfig::try_default(kube).await?,
        })
    }

    pub async fn spawn(&self, kube: &Client, job: AnsibleJob<'_>) -> Result<bool, Error> {
        let ns = crate::consts::NAMESPACE;
        let box_name = job.r#box.spec.machine.uuid.to_string();
        let box_status = job.r#box.status.as_ref();
        let name = format!("box-{}-{}", &job.task, &box_name);

        let bind_group = job
            .r#box
            .status
            .as_ref()
            .and_then(|status| status.bind_group.as_ref());
        let group = &job.r#box.spec.group;
        let reset = self.kiss.group_force_reset || bind_group != Some(&job.r#box.spec.group);

        // delete all previous cronjobs
        {
            let api = Api::<CronJob>::namespaced(kube.clone(), ns);
            let dp = DeleteParams::background();
            let lp = ListParams {
                label_selector: Some(format!("{}={}", AnsibleClient::LABEL_BOX_NAME, &box_name)),
                ..Default::default()
            };
            api.delete_collection(&dp, &lp).await?;
        }
        // delete all previous jobs
        {
            let api = Api::<Job>::namespaced(kube.clone(), ns);
            let dp = DeleteParams::background();
            let lp = ListParams {
                label_selector: Some(format!("{}={}", AnsibleClient::LABEL_BOX_NAME, &box_name)),
                ..Default::default()
            };
            api.delete_collection(&dp, &lp).await?;
        }

        // realize mutual exclusivity (QUEUE)
        let cluster_state = ClusterState::load(kube, &job.r#box.spec).await?;
        if matches!(job.new_state, BoxState::Joining) && !cluster_state.is_joinable() {
            info!(
                "Cluster is not ready: {} {} {} -> {}",
                &job.new_state,
                job.r#box.spec.group.role,
                &box_name,
                &job.r#box.spec.group.cluster_name,
            );
            return Ok(false);
        }

        // define the object
        let metadata = ObjectMeta {
            name: Some(name.clone()),
            namespace: Some(ns.into()),
            labels: Some(
                vec![
                    Some((Self::LABEL_BOX_NAME.into(), box_name.clone())),
                    box_status
                        .and_then(|status| status.access.primary.as_ref())
                        .map(|interface| {
                            (
                                Self::LABEL_BOX_ACCESS_PRIMARY_ADDRESS.into(),
                                interface.address.to_string(),
                            )
                        }),
                    box_status
                        .and_then(|status| status.access.primary.as_ref())
                        .and_then(|interface| interface.speed_mbps)
                        .map(|primary_speed_mbps| {
                            (
                                Self::LABEL_BOX_ACCESS_PRIMART_SPEED_MBPS.into(),
                                primary_speed_mbps.to_string(),
                            )
                        }),
                    Some((
                        Self::LABEL_BOX_MACHINE_UUID.into(),
                        job.r#box.spec.machine.uuid.to_string(),
                    )),
                    Some(("serviceType".into(), "ansible-task".to_string())),
                    job.completed_state
                        .as_ref()
                        .map(ToString::to_string)
                        .map(|state| (Self::LABEL_COMPLETED_STATE.into(), state)),
                    Some((
                        Self::LABEL_GROUP_CLUSTER_NAME.into(),
                        group.cluster_name.clone(),
                    )),
                    Some((Self::LABEL_GROUP_ROLE.into(), group.role.to_string())),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            ..Default::default()
        };
        let spec = JobSpec {
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: metadata.labels.clone(),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    restart_policy: Some("OnFailure".into()),
                    service_account: Some("ansible-playbook".into()),
                    containers: vec![Container {
                        name: "ansible".into(),
                        image: Some(self.kiss.kubespray_image.clone()),
                        command: Some(vec!["ansible-playbook".into()]),
                        args: Some(vec![
                            "--become".into(),
                            "--become-user=root".into(),
                            "--inventory".into(),
                            "/root/ansible/defaults/defaults.yaml".into(),
                            "--inventory".into(),
                            "/root/ansible/defaults/all.yaml".into(),
                            "--inventory".into(),
                            "/root/ansible/config.yaml".into(),
                            "--inventory".into(),
                            "/root/ansible/hosts.yaml".into(),
                            format!(
                                "/opt/playbook/playbook-{}.yaml",
                                group.role.to_string().to_snake_case(),
                            ),
                        ]),
                        env: Some(vec![
                            EnvVar {
                                name: "ansible_host".into(),
                                value: Some(job.r#box.spec.machine.hostname()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "ansible_host_id".into(),
                                value: Some(box_name.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "ansible_host_uuid".into(),
                                value: Some(job.r#box.spec.machine.uuid.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "ansible_ssh_host".into(),
                                value: box_status
                                    .and_then(|status| status.access.management())
                                    .map(|interface| interface.address.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "ansible_ssh_private_key_file".into(),
                                value: Some("/root/.ssh/id_ed25519".into()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "ansible_user".into(),
                                value_from: Some(EnvVarSource {
                                    config_map_key_ref: Some(ConfigMapKeySelector {
                                        name: Some("kiss-config".into()),
                                        key: "auth_ssh_username".into(),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_allow_critical_commands".into(),
                                value: Some(self.kiss.allow_critical_commands.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_allow_pruning_network_interfaces".into(),
                                value: Some(self.kiss.allow_pruning_network_interfaces.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_cluster_control_planes".into(),
                                value: Some(cluster_state.get_control_planes_as_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_cluster_etcd_nodes".into(),
                                value: Some(cluster_state.get_etcd_nodes_as_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_cluster_name".into(),
                                value: Some(group.cluster_name.clone()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_cluster_name_snake_case".into(),
                                value: Some(group.cluster_name.to_snake_case()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_cluster_domain".into(),
                                value: Some(group.cluster_domain()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_cluster_is_default".into(),
                                value: Some(group.is_default().to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_group_enable_default_cluster".into(),
                                value: Some(self.kiss.group_enable_default_cluster.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_group_force_reset".into(),
                                value: Some(reset.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_group_force_reset_os".into(),
                                value: Some(self.kiss.group_force_reset_os.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_group_role".into(),
                                value: Some(group.role.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_interface_mtu_size".into(),
                                value: Some(self.kiss.network_interface_mtu_size.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_dhcp_duration".into(),
                                value: Some(self.kiss.network_ipv4_dhcp_duration.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_dhcp_range_begin".into(),
                                value: Some(self.kiss.network_ipv4_dhcp_range_begin.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_dhcp_range_end".into(),
                                value: Some(self.kiss.network_ipv4_dhcp_range_end.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_gateway".into(),
                                value: Some(self.kiss.network_ipv4_gateway.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_subnet".into(),
                                value: Some(self.kiss.network_ipv4_subnet.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_subnet_address".into(),
                                value: Some(self.kiss.network_ipv4_subnet.network().to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_subnet_mask".into(),
                                value: Some(self.kiss.network_ipv4_subnet.netmask().to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_ipv4_subnet_mask_prefix".into(),
                                value: Some(self.kiss.network_ipv4_subnet.prefix_len().to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_network_nameserver_incluster_ipv4".into(),
                                value: Some(
                                    self.kiss.network_nameserver_incluster_ipv4.to_string(),
                                ),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_power_ipmi_host".into(),
                                value: job
                                    .r#box
                                    .spec
                                    .power
                                    .as_ref()
                                    .map(|power| match power {
                                        BoxPowerSpec::Ipmi { address } => address,
                                    })
                                    .map(|address| address.to_string()),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_power_ipmi_username".into(),
                                value_from: Some(EnvVarSource {
                                    secret_key_ref: Some(SecretKeySelector {
                                        name: Some("kiss-config".into()),
                                        key: "power_ipmi_username".into(),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            EnvVar {
                                name: "kiss_power_ipmi_password".into(),
                                value_from: Some(EnvVarSource {
                                    secret_key_ref: Some(SecretKeySelector {
                                        name: Some("kiss-config".into()),
                                        key: "power_ipmi_password".into(),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        ]),
                        resources: Some(Self::default_resources()),
                        volume_mounts: Some(vec![
                            VolumeMount {
                                name: "ansible".into(),
                                mount_path: "/root/ansible".into(),
                                ..Default::default()
                            },
                            VolumeMount {
                                name: "ansible-defaults".into(),
                                mount_path: "/root/ansible/defaults".into(),
                                ..Default::default()
                            },
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
                            name: "ansible".into(),
                            config_map: Some(ConfigMapVolumeSource {
                                name: Some(format!(
                                    "ansible-control-planes-{}",
                                    &group.cluster_name,
                                )),
                                default_mode: Some(0o400),
                                optional: Some(true),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        Volume {
                            name: "ansible-defaults".into(),
                            config_map: Some(ConfigMapVolumeSource {
                                name: Some("ansible-control-planes-default".into()),
                                default_mode: Some(0o400),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        Volume {
                            name: "playbook".into(),
                            config_map: Some(ConfigMapVolumeSource {
                                name: Some("ansible-task-common".into()),
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
                                secret_name: Some("kiss-config".into()),
                                default_mode: Some(0o400),
                                items: Some(vec![KeyToPath {
                                    key: "auth_ssh_key_id_ed25519".into(),
                                    path: "id_ed25519".into(),
                                    ..Default::default()
                                }]),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ]),
                    ..Default::default()
                }),
            },
            ..Default::default()
        };
        let pp = PostParams {
            dry_run: false,
            field_manager: Some("kube-controller".into()),
        };

        match job.cron {
            Some(schedule) => {
                let api = Api::<CronJob>::namespaced(kube.clone(), ns);
                let job = CronJob {
                    metadata: metadata.clone(),
                    spec: Some(CronJobSpec {
                        concurrency_policy: Some(if job.is_atomic {
                            "Forbid".into()
                        } else {
                            "Replace".into()
                        }),
                        schedule: schedule.into(),
                        starting_deadline_seconds: Some(180 /* 3m */),
                        job_template: JobTemplateSpec {
                            metadata: Some(metadata),
                            spec: Some(spec),
                        },
                        ..Default::default()
                    }),
                    status: None,
                };
                api.create(&pp, &job).await?;
            }
            None => {
                let api = Api::<Job>::namespaced(kube.clone(), ns);
                let job = Job {
                    metadata,
                    spec: Some(spec),
                    status: None,
                };
                api.create(&pp, &job).await?;
            }
        }

        info!("spawned a job: {name}");
        Ok(true)
    }

    pub fn default_resources() -> ResourceRequirements {
        ResourceRequirements {
            requests: Some(
                vec![
                    ("cpu".into(), Quantity("30m".into())),
                    ("memory".into(), Quantity("20Mi".into())),
                ]
                .into_iter()
                .collect(),
            ),
            limits: Some(
                vec![
                    ("cpu".into(), Quantity("50m".into())),
                    ("memory".into(), Quantity("100Mi".into())),
                ]
                .into_iter()
                .collect(),
            ),
        }
    }
}

pub struct AnsibleJob<'a> {
    pub cron: Option<&'static str>,
    pub is_atomic: bool,
    pub task: &'static str,
    pub r#box: &'a BoxCrd,
    pub new_state: BoxState,
    pub completed_state: Option<BoxState>,
}
