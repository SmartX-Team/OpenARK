use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    vm::{
        NetworkVirtualMachine, NetworkVirtualMachineFallbackPolicy,
        NetworkVirtualMachineRestartPolicy,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkArgs {
    #[command(flatten)]
    #[serde(default)]
    pub analyzer: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::Analyzer as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub dependency_graph: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::DependencySolver as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub graph_db: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::GraphDB as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub resource_db: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::ResourceDB as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub runner: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::Runner as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub solver: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::Solver as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub visualizer: <<crate::NetworkVirtualMachine as NetworkVirtualMachine>::Visualizer as NetworkComponent>::Args,

    #[command(flatten)]
    #[serde(default)]
    pub vm: NetworkVirtualMachineArgs,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkVirtualMachineArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_VM_FALLBACK_POLICY",
        value_name = "POLICY",
        default_value_t = NetworkVirtualMachineFallbackPolicy::default(),
    )]
    #[serde(default)]
    pub fallback_policy: NetworkVirtualMachineFallbackPolicy,

    #[arg(
        long,
        env = "KUBEGRAPH_VM_RESTART_POLICY",
        value_name = "POLICY",
        default_value_t = NetworkVirtualMachineRestartPolicy::default(),
    )]
    #[serde(default)]
    pub restart_policy: NetworkVirtualMachineRestartPolicy,
}
