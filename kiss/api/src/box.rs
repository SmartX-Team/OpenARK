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
        "name": "address",
        "type": "string",
        "description":"access address of the box",
        "jsonPath":".status.access.primary.address"
    }"#,
    printcolumn = r#"{
        "name": "power",
        "type": "string",
        "description":"power address of the box",
        "jsonPath":".spec.power.address"
    }"#,
    printcolumn = r#"{
        "name": "cluster",
        "type": "string",
        "description":"cluster name where the box is located",
        "jsonPath":".spec.group.clusterName"
    }"#,
    printcolumn = r#"{
        "name": "role",
        "type": "string",
        "description":"role of the box",
        "jsonPath":".spec.group.role"
    }"#,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description":"state of the box",
        "jsonPath":".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time of the box",
        "jsonPath":".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "updated-at",
        "type": "date",
        "description":"updated time of the box",
        "jsonPath":".status.lastUpdated"
    }"#,
    printcolumn = r#"{
        "name": "network-speed",
        "type": "string",
        "description":"network interface link speed (Unit: Mb/s)",
        "jsonPath":".status.access.primary.speedMbps"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct BoxSpec {
    pub group: BoxGroupSpec,
    pub machine: BoxMachineSpec,
    pub power: Option<BoxPowerSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxStatus {
    pub state: BoxState,
    pub access: BoxAccessSpec,
    pub bind_group: Option<BoxGroupSpec>,
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
            Self::Failed | Self::Disconnected | Self::Missing => Some("reset"),
        }
    }

    pub const fn is_atomic(&self) -> bool {
        matches!(
            self,
            Self::New
                | Self::Commissioning
                | Self::Ready
                | Self::Joining
                | Self::Failed
                | Self::Disconnected,
        )
    }

    pub const fn cron(&self) -> Option<&'static str> {
        match self {
            // Self::Running => Some("0 * * * *"),
            Self::Running => Some("*/5 * * * *"),
            // Self::Disconnected | Self::Missing => Some("@hourly"),
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

    pub fn timeout_new() -> Duration {
        Duration::seconds(30)
    }

    pub fn timeout_update() -> Duration {
        Duration::seconds(15)
    }

    pub fn timeout(&self) -> Option<Duration> {
        let fallback_update = Duration::hours(2);
        let fallback_disconnected = Duration::weeks(1);

        match self {
            Self::New => None,
            Self::Commissioning => Some(fallback_update),
            Self::Ready => None,
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
            Self::New => Self::Missing,
            Self::Commissioning => Self::Failed,
            Self::Ready => Self::Failed,
            Self::Joining => Self::Failed,
            Self::Running => Self::Disconnected,
            Self::Failed => Self::Missing,
            Self::Disconnected => Self::Missing,
            Self::Missing => Self::Missing,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxAccessSpec<Interface = BoxAccessInterfaceSpec> {
    pub primary: Option<Interface>,
}

impl<T> Default for BoxAccessSpec<T> {
    fn default() -> Self {
        Self {
            primary: Default::default(),
        }
    }
}

impl BoxAccessSpec {
    pub fn management(&self) -> Option<&BoxAccessInterfaceSpec> {
        self.primary.as_ref()
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxAccessInterfaceSpec {
    pub address: IpAddr,
    // Speed (Mb/s)
    pub speed_mbps: Option<u64>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct BoxGroupSpec {
    pub cluster_name: String,
    pub role: BoxGroupRole,
}

impl Default for BoxGroupSpec {
    fn default() -> Self {
        Self {
            cluster_name: Self::DEFAULT_CLUSTER_NAME.into(),
            role: BoxGroupRole::default(),
        }
    }
}

impl BoxGroupSpec {
    const DEFAULT_CLUSTER_NAME: &'static str = "default";

    pub fn is_default(&self) -> bool {
        self.cluster_name == Self::DEFAULT_CLUSTER_NAME
    }

    pub fn cluster_domain(&self) -> String {
        if self.is_default() {
            "netai-cloud".into()
        } else {
            format!("{}.netai-cloud", &self.cluster_name)
        }
    }
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
pub enum BoxGroupRole {
    ControlPlane,
    Worker,
}

impl Default for BoxGroupRole {
    fn default() -> Self {
        Self::Worker
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoxMachineSpec {
    pub uuid: Uuid,
}

impl BoxMachineSpec {
    pub fn hostname(&self) -> String {
        self.uuid.to_string()
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
    #[serde(rename_all = "camelCase")]
    pub struct BoxAccessInterfaceQuery {
        pub address: IpAddr,
        // Speed (Mb/s)
        pub speed_mbps: Option<String>,
    }

    impl TryFrom<BoxAccessInterfaceQuery> for BoxAccessInterfaceSpec {
        type Error = <u64 as ::core::str::FromStr>::Err;

        fn try_from(value: BoxAccessInterfaceQuery) -> Result<Self, Self::Error> {
            Ok(Self {
                address: value.address,
                speed_mbps: value.speed_mbps.map(|speed| speed.parse()).transpose()?,
            })
        }
    }

    impl TryFrom<BoxAccessSpec<BoxAccessInterfaceQuery>> for BoxAccessSpec<BoxAccessInterfaceSpec> {
        type Error = <u64 as ::core::str::FromStr>::Err;

        fn try_from(value: BoxAccessSpec<BoxAccessInterfaceQuery>) -> Result<Self, Self::Error> {
            Ok(Self {
                primary: value.primary.map(TryInto::try_into).transpose()?,
            })
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
    pub struct BoxNewQuery {
        #[serde(flatten)]
        pub access_primary: BoxAccessInterfaceQuery,
        #[serde(flatten)]
        pub machine: BoxMachineSpec,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
    pub struct BoxCommissionQuery {
        pub access: BoxAccessSpec<BoxAccessInterfaceQuery>,
        pub machine: BoxMachineSpec,
        pub power: Option<BoxPowerSpec>,
        pub reset: bool,
    }
}
