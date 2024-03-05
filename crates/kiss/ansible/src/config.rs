use std::net::Ipv4Addr;

use anyhow::{anyhow, Error, Result};
use ipnet::Ipv4Net;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};
use tracing::{instrument, Level};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KissConfig {
    pub allow_critical_commands: bool,
    pub allow_pruning_network_interfaces: bool,
    pub bootstrapper_node_size: usize,
    pub etcd_nodes_max: usize,
    pub group_enable_default_cluster: bool,
    pub group_enforce_ansible_control_planes: bool,
    pub group_force_reset: bool,
    pub group_force_reset_os: bool,
    pub kubespray_image: String,
    pub network_interface_mtu_size: u16,
    pub network_ipv4_dhcp_duration: String,
    pub network_ipv4_dhcp_range_begin: Ipv4Addr,
    pub network_ipv4_dhcp_range_end: Ipv4Addr,
    pub network_ipv4_gateway: Ipv4Addr,
    pub network_ipv4_subnet: Ipv4Net,
    pub network_nameserver_incluster_ipv4: Ipv4Addr,
    pub os_default: String,
}

impl KissConfig {
    #[instrument(level = Level::INFO, skip(kube), err(Display))]
    pub async fn try_default(kube: &Client) -> Result<Self> {
        let ns = ::kiss_api::consts::NAMESPACE;
        let api = Api::<ConfigMap>::namespaced(kube.clone(), ns);
        let config = api.get("kiss-config").await?;

        Ok(Self {
            allow_critical_commands: infer(&config, "allow_critical_commands")?,
            allow_pruning_network_interfaces: infer(&config, "allow_pruning_network_interfaces")?,
            bootstrapper_node_size: infer(&config, "bootstrapper_node_size")?,
            etcd_nodes_max: infer(&config, "etcd_nodes_max")?,
            group_enable_default_cluster: infer(&config, "group_enable_default_cluster")?,
            group_enforce_ansible_control_planes: infer(
                &config,
                "group_enforce_ansible_control_planes",
            )?,
            group_force_reset: infer(&config, "group_force_reset")?,
            group_force_reset_os: infer(&config, "group_force_reset_os")?,
            kubespray_image: infer(&config, "kubespray_image")?,
            network_interface_mtu_size: infer(&config, "network_interface_mtu_size")?,
            network_ipv4_dhcp_duration: infer(&config, "network_ipv4_dhcp_duration")?,
            network_ipv4_dhcp_range_begin: infer(&config, "network_ipv4_dhcp_range_begin")?,
            network_ipv4_dhcp_range_end: infer(&config, "network_ipv4_dhcp_range_end")?,
            network_ipv4_gateway: infer(&config, "network_ipv4_gateway")?,
            network_ipv4_subnet: infer(&config, "network_ipv4_subnet")?,
            network_nameserver_incluster_ipv4: infer(&config, "network_nameserver_incluster_ipv4")?,
            os_default: infer(&config, "os_default")?,
        })
    }
}

pub fn infer<K: AsRef<str>, R>(config: &ConfigMap, key: K) -> Result<R>
where
    R: ::core::str::FromStr,
    <R as ::core::str::FromStr>::Err: Into<Error> + Send + Sync + 'static,
{
    let key = key.as_ref();

    config
        .data
        .as_ref()
        .and_then(|data| data.get(key))
        .ok_or_else(|| anyhow!("failed to find the configuration variable: {key}"))
        .and_then(|e| e.parse().map_err(Into::into))
}
