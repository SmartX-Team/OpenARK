use clap::Parser;
use kubegraph_api::vm::{NetworkVirtualMachineFallbackPolicy, NetworkVirtualMachineRestartPolicy};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
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
