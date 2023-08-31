#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Remove temporary user space
sudo rm -rf /opt/vdi/tenants/remote/vine-session-*

# Grow root partition size
if ip a | grep wlp >/dev/null 2>/dev/null; then
    if df -h | grep /dev/nvme0n1p3 | grep -Po '^/dev/nvme0n1p3 *200G' >/dev/null 2>/dev/null; then
        sudo dnf install -y cloud-utils-growpart
        sudo growpart /dev/nvme0n1 3
        sudo resize2fs /dev/nvme0n1p3
    fi
fi

# Allow containerd to pull images from insecure local private registry
if ! sudo cat /etc/containerd/config.toml | grep 'registry.ark.svc.ops.openark' >/dev/null 2>/dev/null; then
    echo '        [plugins."io.containerd.grpc.v1.cri".registry.mirrors."registry.ark.svc.ops.openark"]' |
        sudo tee -a /etc/containerd/config.toml
    echo '          endpoint = ["http://registry.ark.svc.ops.openark"]' |
        sudo tee -a /etc/containerd/config.toml
    echo '      [plugins."io.containerd.grpc.v1.cri".registry.configs]' |
        sudo tee -a /etc/containerd/config.toml
    echo '        [plugins."io.containerd.grpc.v1.cri".registry.configs."registry.ark.svc.ops.openark"]' |
        sudo tee -a /etc/containerd/config.toml
    echo '          [plugins."io.containerd.grpc.v1.cri".registry.configs."registry.ark.svc.ops.openark".tls]' |
        sudo tee -a /etc/containerd/config.toml
    echo '            insecure_skip_verify = true' |
        sudo tee -a /etc/containerd/config.toml
fi

# Restart containerd if Mobile
if ip a | grep wlp >/dev/null 2>/dev/null; then
    sudo systemctl restart containerd
fi
