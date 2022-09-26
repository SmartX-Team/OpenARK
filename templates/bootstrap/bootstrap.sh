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
KUBERNETES_DATA_DEFAULT="/opt/kiss"
KUBESPRAY_CONFIG_DEFAULT="$(pwd)/config/bootstrap/defaults/all.yaml"
KUBESPRAY_CONFIG_TEMPLATE_DEFAULT="$(pwd)/config/"
KUBESPRAY_IMAGE_DEFAULT="quay.io/kubespray/kubespray:v2.19.1"
KUBESPRAY_NODES_DEFAULT="master1"
REUSE_DATA_DEFAULT="false"
REUSE_NODES_DEFAULT="true"
SSH_KEYFILE_DEFAULT="$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT/id_rsa"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
KISS_BOOTSTRAP_NODE_IMAGE="${KISS_BOOTSTRAP_NODE_IMAGE:-$KISS_BOOTSTRAP_NODE_IMAGE_DEFAULT}"
KISS_INSTALLER_IMAGE="${KISS_INSTALLER_IMAGE:-$KISS_INSTALLER_IMAGE_DEFAULT}"
KUBERNETES_CONFIG="${KUBERNETES_CONFIG:-$KUBERNETES_CONFIG_DEFAULT}"
KUBERNETES_DATA="${KUBERNETES_DATA:-$KUBERNETES_DATA_DEFAULT}"
KUBESPRAY_CONFIG="${KUBESPRAY_CONFIG:-$KUBESPRAY_CONFIG_DEFAULT}"
KUBESPRAY_CONFIG_TEMPLATE="${KUBESPRAY_CONFIG_TEMPLATE:-$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT}"
KUBESPRAY_IMAGE="${KUBESPRAY_IMAGE:-$KUBESPRAY_IMAGE_DEFAULT}"
KUBESPRAY_NODES="${KUBESPRAY_NODES:-$KUBESPRAY_NODES_DEFAULT}"
REUSE_DATA="${REUSE_DATA:-$REUSE_DATA_DEFAULT}"
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
    echo "- Generating a SSH Keypair ... "
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
    if [ $("$CONTAINER_RUNTIME" ps -a -q -f "name=$name") ]; then
        if [ $(echo "$REUSE_NODES" | awk '{print tolower($0)}') == "true" ]; then
            echo -n "- Using already spawned node ($name) ... "
            local NEED_SPAWN=0
        else
            echo "Error: Already spawned node ($name)"
            exit 1
        fi
    fi

    if [ "$NEED_SPAWN" -eq 1 ]; then
        # Reset data
        if [ $(echo "$REUSE_DATA" | awk '{print tolower($0)}') == "false" ]; then
            echo "- Removing previous data ... "
            sudo rm -rf "$KUBERNETES_DATA"
            sudo mkdir -p "$KUBERNETES_DATA"
        fi

        # Spawn a node
        echo -n "- Spawning a node ($name) ... "
        "$CONTAINER_RUNTIME" run --detach \
            --name "$name" \
            --cgroupns "host" \
            --hostname "$name" \
            --ipc "host" \
            --net "host" \
            --privileged \
            --env "SSH_PUBKEY=$(cat ${SSH_KEYFILE}.pub)" \
            --tmpfs "/run" \
            --volume "/lib/modules/$UNAME:/lib/modules/$UNAME:ro" \
            --volume "$KUBERNETES_DATA/binary.cni:/opt/cni:shared" \
            --volume "$KUBERNETES_DATA/binary.common:/usr/local/bin:shared" \
            --volume "$KUBERNETES_DATA/binary.etcd:/opt/etcd:shared" \
            --volume "$KUBERNETES_DATA/binary.pypy3:/opt/pypy3:shared" \
            --volume "$KUBERNETES_DATA/var.cni:/var/lib/cni:shared" \
            --volume "$KUBERNETES_DATA/var.containerd:/var/lib/containerd:shared" \
            --volume "$KUBERNETES_DATA/var.k8s:/var/lib/kubelet:shared" \
            --volume "/sys/fs/cgroup:/sys/fs/cgroup" \
            "$KISS_BOOTSTRAP_NODE_IMAGE" >/dev/null
    else
        # Start SSH
        "$CONTAINER_RUNTIME" exec "$name" systemctl start sshd
    fi

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

    # Update SSH ListenAddress
    "$CONTAINER_RUNTIME" exec "$name" sed -i \
        "s/^\(ListenAddress\) .*\$/\1 $node_ip/g" \
        /etc/ssh/sshd_config

    # Restart SSH daemon
    while [ ! $(
        "$CONTAINER_RUNTIME" exec -it $name ps -s 1 |
            awk '{print $4}' |
            tail -n 1 |
            grep '^systemd'
    ) ]; do
        sleep 1
    done
    "$CONTAINER_RUNTIME" exec "$name" \
        systemctl restart sshd 2>/dev/null || true

    # Get SSH configuration
    while :; do
        # Get SSH port
        local SSH_PORT="$(
            "$CONTAINER_RUNTIME" exec "$name" cat /etc/ssh/sshd_config |
                grep '^Port ' |
                awk '{print $2}'
        )"

        # Try connect to the node
        if
            ssh \
                -o "StrictHostKeyChecking=no" \
                -o "UserKnownHostsFile=/dev/null" \
                -p $SSH_PORT \
                -i $SSH_KEYFILE \
                "root@$node_ip" \
                exit \
                2>/dev/null
        then
            break
        fi
    done

    # Save as environment variable
    local node="$name:$node_ip:$SSH_PORT"
    export nodes="$nodes $node"

    # Finished!
    echo "OK ($node)"
}

# Spawn a node
export nodes # results
for name in $KUBESPRAY_NODES; do
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
            kubectl get nodes --no-headers "$node_first.ops.netai-cloud" \
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
        for name in $KUBESPRAY_NODES; do
            "$CONTAINER_RUNTIME" exec "$node_first" \
                mkdir -p "/root/kiss/bootstrap/"
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                tee "/root/kiss/bootstrap/config.yaml" \
                <"$KUBESPRAY_CONFIG" |
                echo -n ''
        done

        # Download k8s config into host
        mkdir -p "$KUBERNETES_CONFIG"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            tar -cf - -C "/root/.kube" "." |
            tar -xf - -C "$KUBERNETES_CONFIG"

        # Cleanup
        rm -rf "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"
    fi

    # Finished!
    echo "OK"
}

# Install a k8s cluster within nodes
install_k8s_cluster $KUBESPRAY_NODES

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
        # Upload the K8S Configuration File to the Cluster
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create namespace kiss
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "ansible-control-planes-default" \
            "--from-file=all.yaml=/etc/kiss/bootstrap/defaults/all.yaml" \
            "--from-file=hosts.yaml=/etc/kiss/bootstrap/inventory/hosts.yaml" \
            "--from-file=config.yaml=/root/kiss/bootstrap/config.yaml"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "ansible-images" \
            "--from-literal=kubespray=$KUBESPRAY_IMAGE"

        # Upload the SSH Configuration File to the Cluster
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "matchbox-account" \
            "--from-literal=username=kiss" \
            "--from-literal=id_rsa.pub=$(
                cat ${SSH_KEYFILE}.pub |
                    awk '{print $1 " " $2}'
            )"
        "$CONTAINER_RUNTIME" cp "${SSH_KEYFILE}" "$node_first:/tmp/kiss_bootstrap_id_rsa"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss secret generic "matchbox-account" \
            "--from-file=id_rsa=/tmp/kiss_bootstrap_id_rsa" ||
            "$CONTAINER_RUNTIME" exec "$node_first" \
                rm -f "/tmp/kiss_bootstrap_id_rsa"

        # Install cluster
        echo "- Installing kiss cluster ... "
        "$CONTAINER_RUNTIME" run --rm \
            --name "kiss-installer" \
            --net "host" \
            --volume "$KUBERNETES_CONFIG:/root/.kube:ro" \
            "$KISS_INSTALLER_IMAGE"
    fi

    # Finished!
    echo "OK"
}

# Install a kiss cluster within k8s cluster
install_kiss_cluster $KUBESPRAY_NODES

###########################################################
#   Install k8s snapshot config                           #
###########################################################

# Define a k8s snapshot config installer function
function install_k8s_snapshot_cluster() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Check if k8s snapshot config already exists
    local NEED_INSTALL=1
    if
        true &&
            "$CONTAINER_RUNTIME" exec "$node_first" \
                kubectl get -n kiss configmap "snapshot-account-git" \
                >/dev/null 2>/dev/null &&
            "$CONTAINER_RUNTIME" exec "$node_first" \
                kubectl get -n kiss secret "snapshot-account-git" \
                >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed k8s snapshot config ... "
        local NEED_INSTALL=0

    # Check if k8s snapshot config is given
    elif [ "$SNAPSHOT_GIT_REPOSITORY" == "" ]; then
        echo -n "- Skipping installing k8s snapshot config - "
        echo -n "No such environment variable: 'SNAPSHOT_GIT_REPOSITORY' ... "
        local NEED_INSTALL=0
    fi

    if [ "$NEED_INSTALL" -eq 1 ]; then
        # Show how to deploy your SSH keys into the Web (i.e. Github) repository.
        echo
        echo "* NOTE: You can register the SSH public key to activate the snapshot manager."
        echo "* Your SSH key: \"$(
            cat ${SSH_KEYFILE}.pub |
                awk '{print $1 " " $2}'
        )\""
        echo "* Your SSH key is saved on: \"${SSH_KEYFILE}.pub\""
        echo "* Learn How to store keys (Github): \"https://docs.github.com/en/developers/overview/managing-deploy-keys#deploy-keys\""
        echo

        echo "- Installing k8s snapshot manager ... "

        # Upload the K8S Snapshot Configuration File to the Cluster
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "snapshot-git" \
            "--from-literal=repository=$SNAPSHOT_GIT_REPOSITORY"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get -n kiss secret "matchbox-account" \
            -o jsonpath='{.data.id_rsa}' |
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                base64 --decode |
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                tee "/tmp/kiss_snapshot_id_rsa" |
            echo -n ''
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss secret generic "snapshot-git" \
            "--from-file=id_rsa=/tmp/kiss_snapshot_id_rsa" ||
            "$CONTAINER_RUNTIME" exec "$node_first" \
                rm -f "/tmp/kiss_snapshot_id_rsa"
    fi

    # Finished!
    echo "OK"
}

# Install a k8s snapshot config within k8s cluster
install_k8s_snapshot_cluster $KUBESPRAY_NODES

# Finished!
echo "Installed!"
