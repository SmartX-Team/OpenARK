use std::net::IpAddr;

use ipis::core::{
    chrono::{DateTime, Utc},
    uuid::Uuid,
};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::Display;

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
        "name": "State",
        "type": "string",
        "description":"state of the box",
        "jsonPath":".status.state"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct BoxSpec {
    pub access: BoxAccessSpec,
    pub machine: BoxMachineSpec,
    pub power: Option<BoxPowerSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxStatus {
    pub state: BoxState,
    pub last_updated: DateTime<Utc>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
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
    Disconnected,
    Reconnecting,
    Missing,
    Failed,
    Resetting,
}

impl BoxState {
    pub fn as_task(&self) -> Option<&'static str> {
        match self {
            Self::New => None,
            Self::Commissioning => Some("commission"),
            Self::Ready => None,
            Self::Joining => Some("join"),
            Self::Running => None,
            Self::Disconnected => None,
            Self::Reconnecting => Some("reconnect"),
            Self::Missing => None,
            Self::Failed => None,
            Self::Resetting => Some("reset"),
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::New => Self::Commissioning,
            Self::Commissioning => Self::Commissioning,
            Self::Ready => Self::Joining,
            Self::Joining => Self::Joining,
            Self::Running => Self::Running,
            Self::Disconnected => Self::Reconnecting,
            Self::Reconnecting => Self::Reconnecting,
            Self::Missing => Self::Missing,
            Self::Failed => Self::Resetting,
            Self::Resetting => Self::Resetting,
        }
    }

    pub fn complete(&self) -> Option<Self> {
        match self {
            Self::New => None,
            Self::Commissioning => Some(Self::Ready),
            Self::Ready => None,
            Self::Joining => Some(Self::Running),
            Self::Running => None,
            Self::Disconnected => None,
            Self::Reconnecting => None,
            Self::Missing => None,
            Self::Failed => None,
            Self::Resetting => None,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BoxPowerSpec {
    IPMI { address: IpAddr },
}
