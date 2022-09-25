# GitOps Management

It help you automatically reflect cluster information written with YAML through Git.

## Install

1. Store the repository's SSH path into your root cluster.
1. Store the root cluster's kiss SSH keys into [`deploy keys`](https://docs.github.com/en/developers/overview/managing-deploy-keys#deploy-keys).
1. Save the configuration about boxes and clusters like belows.

### How to manage Boxes?

Path: `/boxes.yaml`

```yaml
all:
  # Default variables
  vars:
    clusterRole: Worker
  children:
    ##########################################
    # Rack 1
    ##########################################
    rack-1:
      vars:
        rack_id: 1
      hosts:
        rack-1-box-1.ops.netai-cloud:
          uuid: 00000000-0000-0000-0000-000000000000
          ipmi_address: 10.32.123.123
    ...
```

### How to manage Clusters?

Path: `/clusters/my-cluster.yaml`

```yaml
my-cluster:
  vars:
    created_at: 2022-09-26
    updated_at: 2022-09-26
    clusterName: my-cluster
    manager: Ho Kim
  children:
    rack-1-box-1.ops.netai-cloud:
    ...
```
