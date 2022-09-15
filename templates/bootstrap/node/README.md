# Kiss Bootstrap Container Node

This container image is used as the underlying container to deploy the `KISS` control plane using the open-source Kubernetes deployment tool [ `kubespray` ](https://kubespray.io/).

## Why not [*kind*](https://kind.sigs.k8s.io/) ?

`kind` helps run kubernetes clusters within docker containers.
It is obviously optimized for deploying **local** clusters, so it looks useful for PoC workloads. However, the `KISS` cluster requires tasks that go beyond the typical sandboxing level provided by current CRIs, such as DHCP. And the `KISS` cluster automatically creates and expands the cluster using `kubespray` , another open-source Kubernetes distribution tool. These tools are difficult to use at the same time because they are not compatible with each other.
