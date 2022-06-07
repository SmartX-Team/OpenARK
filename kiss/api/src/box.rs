use std::net::IpAddr;

use ipis::core::{
    chrono::{DateTime, Utc},
    uuid::Uuid,
};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum BoxState {
    New,
    Commissioning,
    Ready,
    Joining,
    Running,
    Disconnected,
    Missing,
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
