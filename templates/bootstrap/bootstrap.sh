#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
CONTAINER_RUNTIME_DEFAULT="docker"
KISS_BOOTSTRAP_NODE_IMAGE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-bootstrap-node:latest"
KISS_INSTALLER_IMAGE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-upgrade-kiss:latest"
KUBERNETES_CONFIG_DEFAULT="$HOME/.kube/"
KUBESPRAY_CONFIG_DEFAULT="$(pwd)/config/bootstrap/defaults/all.yaml"
KUBESPRAY_CONFIG_TEMPLATE_DEFAULT="$(pwd)/config/"
KUBESPRAY_IMAGE_DEFAULT="quay.io/kubespray/kubespray:v2.19.1"
KUBESPRAY_NODES_DEFAULT="master1"
REUSE_NODES_DEFAULT="true"
SSH_KEYFILE_DEFAULT="$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT/id_rsa"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
KISS_BOOTSTRAP_NODE_IMAGE="${KISS_BOOTSTRAP_NODE_IMAGE:-$KISS_BOOTSTRAP_NODE_IMAGE_DEFAULT}"
KISS_INSTALLER_IMAGE="${KISS_INSTALLER_IMAGE:-$KISS_INSTALLER_IMAGE_DEFAULT}"
KUBERNETES_CONFIG_DEFAULT="${KUBERNETES_CONFIG_DEFAULT:-$KUBERNETES_CONFIG_DEFAULT_DEFAULT}"
KUBESPRAY_CONFIG="${KUBESPRAY_CONFIG:-$KUBESPRAY_CONFIG_DEFAULT}"
KUBESPRAY_CONFIG_TEMPLATE="${KUBESPRAY_CONFIG_TEMPLATE:-$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT}"
KUBESPRAY_IMAGE="${KUBESPRAY_IMAGE:-$KUBESPRAY_IMAGE_DEFAULT}"
KUBESPRAY_NODES="${KUBESPRAY_NODES:-$KUBESPRAY_NODES_DEFAULT}"
REUSE_NODES="${REUSE_NODES:-$REUSE_NODES_DEFAULT}"
SSH_KEYFILE="${SSH_KEYFILE:-$SSH_KEYFILE_DEFAULT}"

###########################################################
#   Check Dependencies                                    #
###########################################################

# Check linux dependencies
UNAME="$(uname -r)"
if [ ! -d "/lib/modules/$UNAME" ]; then
    echo "Error: Cannot find the linux modules (/lib/modules/$UNAME)"
    echo "Note: You may reboot your machine to reload the kernel."
    exit 1
fi

###########################################################
#   Configure Linux Kernel                                #
###########################################################

# Disable swap
sudo swapoff -a

# Generate a SSH keypair
if [ ! -f "${SSH_KEYFILE_DEFAULT}" ]; then
    echo "Generating a SSH Keypair..."
    mkdir -p "$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT"
    ssh-keygen -q -t rsa -f "$SSH_KEYFILE" -N ''
fi

###########################################################
#   Spawn nodes                                           #
###########################################################

# Define a node spawner function
function spawn_node() {
    local name="$1"

    # Check if node already exists
    local NEED_SPAWN=1
    if [ $(docker ps -a -q -f "name=^$name\$") ]; then
        if [ $(echo "$REUSE_NODES" | awk '{print tolower($0)}') == "true" ]; then
            echo -n "- Using already spawned node ($name) ... "
            local NEED_SPAWN=0
        else
            echo "Error: Already spawned node ($name)"
            exit 1
        fi
    fi

    if [ "$NEED_SPAWN" -eq 1 ]; then
        # Spawn a node
        echo -n "- Spawning a node ($name) ... "
        "$CONTAINER_RUNTIME" run --detach \
            --name "$name" \
            --cgroupns "host" \
            --hostname "$name.control.box.netai-cloud" \
            --ipc "host" \
            --net "host" \
            --privileged \
            --env "SSH_PUBKEY=$(cat ${SSH_KEYFILE}.pub)" \
            --restart "unless-stopped" \
            --tmpfs "/run" \
            --volume "/lib/modules/$UNAME:/lib/modules/$UNAME:ro" \
            --volume "/sys/fs/cgroup:/sys/fs/cgroup" \
            "$KISS_BOOTSTRAP_NODE_IMAGE" >/dev/null
    else
        # Start SSH
        docker exec "$name" systemctl start sshd
    fi

    # Get SSH configuration
    while :; do
        # Get SSH port
        local SSH_PORT="$(docker exec "$name" cat /etc/ssh/sshd_config | grep '^Port ' | awk '{print $2}')"

        # Try connect to the node
        if ssh -o "StrictHostKeyChecking=no" -o "UserKnownHostsFile=/dev/null" -p $SSH_PORT -i $SSH_KEYFILE root@127.0.0.1 exit 2>/dev/null; then
            break
        fi
    done

    # Get suitable access IP
    node_ip=$(
        "$CONTAINER_RUNTIME" exec "$name" ip a |
            grep -o '10\.\(3[2-9]\|4[0-7]\)\(\.\(25[0-5]\|\(2[0-4]\|1[0-9]\|[1-9]\|\)[0-9]\)\)\{2\}' |
            head -1
    )
    if [ ! "$node_ip" ]; then
        echo "Err"
        echo "Error: Cannot find host IP (10.32.0.0/12)"
        exit 1
    fi

    # Save as environment variable
    local node="$name:$node_ip:$SSH_PORT"
    export nodes="$nodes $node"

    # Finished!
    echo "OK ($node)"
}

# Spawn a node
export nodes # results
for name in "$KUBESPRAY_NODES"; do
    spawn_node "$name"
done

###########################################################
#   Install k8s cluster                                   #
###########################################################

# Define a k8s cluster installer function
function install_k8s_cluster() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Check if k8s cluster already exists
    local NEED_INSTALL=1
    if
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get nodes --no-headers "$node_first.control.box.netai-cloud" \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed k8s cluster ... "
        local NEED_INSTALL=0
    fi

    if [ "$NEED_INSTALL" -eq 1 ]; then
        # Cleanup
        rm -rf "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"

        # Get a sample kubespray configuration file
        mkdir -p "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            tar -cf - -C "/etc/kiss/bootstrap/" "." |
            tar -xf - -C "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"

        # Install cluster
        echo "- Installing k8s cluster ... "
        "$CONTAINER_RUNTIME" run --rm \
            --name "k8s-installer" \
            --net "host" \
            --env "KUBESPRAY_NODES=$nodes" \
            --volume "$KUBESPRAY_CONFIG:/root/kiss/bootstrap/config.yaml:ro" \
            --volume "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/:/etc/kiss/bootstrap/:ro" \
            --volume "$SSH_KEYFILE:/root/.ssh/id_rsa:ro" \
            --volume "$SSH_KEYFILE.pub:/root/.ssh/id_rsa.pub:ro" \
            "$KUBESPRAY_IMAGE" ansible-playbook \
            --become --become-user="root" \
            --inventory "/etc/kiss/bootstrap/defaults/all.yaml" \
            --inventory "/root/kiss/bootstrap/config.yaml" \
            "/etc/kiss/bootstrap/roles/install-k8s.yaml"

        # Upload kubespray config into nodes
        for name in "$KUBESPRAY_NODES"; do
            "$CONTAINER_RUNTIME" exec "$node_first" \
                mkdir -p "/root/kiss/bootstrap/"
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                bash -c "cat > /root/kiss/bootstrap/config.yaml" \
                <"$KUBESPRAY_CONFIG"
        done

        # Download k8s config into host
        mkdir -p "$KUBERNETES_CONFIG_DEFAULT"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            tar -cf - -C "/root/.kube" "." |
            tar -xf - -C "$KUBERNETES_CONFIG_DEFAULT"

        # Cleanup
        rm -rf "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"
    fi

    # Finished!
    echo "OK"
}

# Install a k8s cluster within nodes
install_k8s_cluster "$KUBESPRAY_NODES"

###########################################################
#   Install kiss cluster                                  #
###########################################################

# Define a kiss cluster installer function
function install_kiss_cluster() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Check if kiss cluster already exists
    local NEED_INSTALL=1
    if
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get namespaces kiss \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed kiss cluster ... "
        local NEED_INSTALL=0
    fi

    if [ "$NEED_INSTALL" -eq 1 ]; then
        # Upload the Configuration File to the Cluster
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create namespace kiss
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "ansible-control-planes-default" \
            "--from-file=/etc/kiss/bootstrap/defaults/all.yaml" \
            "--from-file=/etc/kiss/bootstrap/inventory/hosts.yaml" \
            "--from-file=/root/kiss/bootstrap/config.yaml"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "ansible-images" \
            "--from-literal=kubespray=$KUBESPRAY_IMAGE"

        # Install cluster
        echo "- Installing kiss cluster ... "
        "$CONTAINER_RUNTIME" run --rm \
            --name "kiss-installer" \
            --net "host" \
            --volume "$KUBERNETES_CONFIG_DEFAULT:/root/.kube:ro" \
            "$KISS_INSTALLER_IMAGE"
    fi

    # Finished!
    echo "OK"
}

# Install a kiss cluster within k8s cluster
install_kiss_cluster "$KUBESPRAY_NODES"

# Finished!
echo "Installed!"
