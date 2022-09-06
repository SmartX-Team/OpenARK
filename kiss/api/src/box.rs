use std::net::IpAddr;

use ipis::core::{
    chrono::{DateTime, Duration, Utc},
    uuid::Uuid,
};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kiss.netai-cloud",
    version = "v1alpha1",
    kind = "Box",
    struct = "BoxCrd",
    status = "BoxStatus",
    shortname = "box",
    printcolumn = r#"{
        "name": "Address",
        "type": "string",
        "description":"access address of the box",
        "jsonPath":".spec.access.address"
    }"#,
    printcolumn = r#"{
        "name": "Power",
        "type": "string",
        "description":"power address of the box",
        "jsonPath":".spec.power.address"
    }"#,
    printcolumn = r#"{
        "name": "Cluster",
        "type": "string",
        "description":"cluster name where the box is located",
        "jsonPath":".spec.cluster"
    }"#,
    printcolumn = r#"{
        "name": "State",
        "type": "string",
        "description":"state of the box",
        "jsonPath":".status.state"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct BoxSpec {
    pub access: BoxAccessSpec,
    pub cluster: Option<String>,
    pub machine: BoxMachineSpec,
    pub power: Option<BoxPowerSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxStatus {
    pub state: BoxState,
    pub bind_cluster: Option<String>,
    pub last_updated: DateTime<Utc>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum BoxState {
    New,
    Commissioning,
    Ready,
    Joining,
    Running,
    Failed,
    Disconnected,
    Missing,
}

impl BoxState {
    pub const fn as_task(&self) -> Option<&'static str> {
        match self {
            Self::New => None,
            Self::Commissioning => Some("commission"),
            Self::Ready => None,
            Self::Joining => Some("join"),
            Self::Running => Some("ping"),
            Self::Failed => None,
            Self::Disconnected => Some("reset"),
            Self::Missing => None,
        }
    }

    pub const fn cron(&self) -> Option<&'static str> {
        match self {
            // Self::Running => Some("0 * * * *"),
            Self::Running => Some("@hourly"),
            _ => None,
        }
    }

    pub const fn next(&self) -> Self {
        match self {
            Self::New => Self::Commissioning,
            Self::Commissioning => Self::Commissioning,
            Self::Ready => Self::Joining,
            Self::Joining => Self::Joining,
            Self::Running => Self::Running,
            Self::Failed => Self::Disconnected,
            Self::Disconnected => Self::Disconnected,
            Self::Missing => Self::Missing,
        }
    }

    pub fn timeout(&self) -> Option<Duration> {
        let fallback_update = Duration::hours(2);
        let fallback_disconnected = Duration::weeks(1);

        match self {
            Self::New => Some(fallback_update),
            Self::Commissioning => Some(fallback_update),
            Self::Ready => Some(fallback_update),
            Self::Joining => Some(fallback_update),
            Self::Running => None,
            Self::Failed => Some(fallback_disconnected),
            Self::Disconnected => Some(fallback_disconnected),
            Self::Missing => None,
        }
    }

    pub const fn complete(&self) -> Option<Self> {
        match self {
            Self::New => None,
            Self::Commissioning => Some(Self::Ready),
            Self::Ready => None,
            Self::Joining => Some(Self::Running),
            Self::Running => None,
            Self::Failed => None,
            Self::Disconnected => None,
            Self::Missing => None,
        }
    }

    pub const fn fail(&self) -> Self {
        match self {
            Self::New => Self::Failed,
            Self::Commissioning => Self::Failed,
            Self::Ready => Self::Failed,
            Self::Joining => Self::Failed,
            Self::Running => Self::Disconnected,
            Self::Failed => Self::Disconnected,
            Self::Disconnected => Self::Missing,
            Self::Missing => Self::Missing,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxAccessSpec {
    pub address: IpAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxMachineSpec {
    pub uuid: Uuid,
}

impl BoxMachineSpec {
    pub fn hostname(&self) -> String {
        format!("{}.box.netai-cloud", &self.uuid)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum BoxPowerSpec {
    #[serde(rename_all = "camelCase")]
    Ipmi { address: IpAddr },
}

pub mod request {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
    pub struct BoxNewQuery {
        #[serde(flatten)]
        pub access: BoxAccessSpec,
        #[serde(flatten)]
        pub machine: BoxMachineSpec,
    }
}
