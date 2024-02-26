# OpenARK Bootstrapper

## Requirements

- `CONTAINER_RUNTIME`: docker
- `DHCP_SERVER` (optional):
  - IP: 10.47.0.1 (`bootstrapper_network_ipv4_address`)
  - Subnet: 10.32.0.0/12 (`network_ipv4_subnet`)
  - DHCP Range: 10.32.0.0/16 (`network_ipv4_dhcp_range_begin` ~ `network_ipv4_dhcp_range_end`)
  - Gateway: 10.47.255.254 (`network_ipv4_gateway`)
  - MTU: 9000 (`network_interface_mtu_size`)
  - Host Nameservers: 1.1.1.1, 1.0.0.1 (`bootstrapper_network_dns_server_ns1` ~ `bootstrapper_network_dns_server_ns2`)
  - Cluster Nameservers: 10.64.0.3 (`network_nameserver_incluster_ipv4`)
  - Options: `dnsmasq` Deployment

## Build an Installer ISO

```bash
env INSTLLAER_TYPE=iso ./bootstrap.sh
```

## Install on Host Machine

```bash
env INSTLLAER_TYPE=container ./bootstrap.sh
```
