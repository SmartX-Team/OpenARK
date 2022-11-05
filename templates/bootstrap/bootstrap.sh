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
BAREMETAL_CSI_DEFAULT="rook-ceph"
BAREMETAL_CSI_INSTALLER_IMAGE_TEMPLATE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-upgrade-csi-__BAREMETAL_CSI__:latest"
BAREMETAL_GPU_INSTALLER_IMAGE_TEMPLATE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-upgrade-gpu-__BAREMETAL_GPU__:latest"
BAREMETAL_GPU_NVIDIA_DEFAULT="true"
CONTAINER_RUNTIME_DEFAULT="docker"
IPIS_ENABLE_DEFAULT="true"
IPIS_INSTALLER_IMAGE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-upgrade-ipis:latest"
KISS_BOOTSTRAP_NODE_IMAGE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-bootstrap-node:latest"
KISS_INSTALLER_IMAGE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-upgrade-kiss:latest"
KUBERNETES_CONFIG_DEFAULT="$HOME/.kube/"
KUBERNETES_DATA_DEFAULT="/opt/kiss"
KUBESPRAY_CONFIG_DEFAULT="$(pwd)/config/bootstrap/defaults/all.yaml"
KUBESPRAY_CONFIG_ALL_DEFAULT="$(pwd)/config/bootstrap/defaults/all.yaml"
KUBESPRAY_CONFIG_TEMPLATE_DEFAULT="$(pwd)/config/"
KUBESPRAY_IMAGE_DEFAULT="quay.io/ulagbulag-village/kubespray:latest"
KUBESPRAY_NODES_DEFAULT="node1.master"
REUSE_DATA_DEFAULT="false"
REUSE_NODES_DEFAULT="true"
SNAPSHOT_GIT_BRANCH_DEFAULT="master"
SNAPSHOT_GIT_REPOSITORY_DEFAULT=""
SNAPSHOT_GIT_USER_EMAIL_DEFAULT="kiss.bot@ulagbulag.io"
SNAPSHOT_GIT_USER_NAME_DEFAULT="NetAI Cloud KISS BOT"
SSH_KEYFILE_DEFAULT="$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT/id_rsa"

# Configure environment variables
BAREMETAL_CSI="${BAREMETAL_CSI:-$BAREMETAL_CSI_DEFAULT}"
BAREMETAL_CSI_INSTALLER_IMAGE_TEMPLATE="${BAREMETAL_CSI_INSTALLER_IMAGE_TEMPLATE:-$BAREMETAL_CSI_INSTALLER_IMAGE_TEMPLATE_DEFAULT}"
BAREMETAL_GPU_INSTALLER_IMAGE_TEMPLATE="${BAREMETAL_GPU_INSTALLER_IMAGE_TEMPLATE:-$BAREMETAL_GPU_INSTALLER_IMAGE_TEMPLATE_DEFAULT}"
BAREMETAL_GPU_NVIDIA="${BAREMETAL_GPU_NVIDIA:-$BAREMETAL_GPU_NVIDIA_DEFAULT}"
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
IPIS_ENABLE="${IPIS_ENABLE:-$IPIS_ENABLE_DEFAULT}"
IPIS_INSTALLER_IMAGE="${IPIS_INSTALLER_IMAGE:-$IPIS_INSTALLER_IMAGE_DEFAULT}"
KISS_BOOTSTRAP_NODE_IMAGE="${KISS_BOOTSTRAP_NODE_IMAGE:-$KISS_BOOTSTRAP_NODE_IMAGE_DEFAULT}"
KISS_INSTALLER_IMAGE="${KISS_INSTALLER_IMAGE:-$KISS_INSTALLER_IMAGE_DEFAULT}"
KUBERNETES_CONFIG="${KUBERNETES_CONFIG:-$KUBERNETES_CONFIG_DEFAULT}"
KUBERNETES_DATA="${KUBERNETES_DATA:-$KUBERNETES_DATA_DEFAULT}"
KUBESPRAY_CONFIG="${KUBESPRAY_CONFIG:-$KUBESPRAY_CONFIG_DEFAULT}"
KUBESPRAY_CONFIG_ALL="${KUBESPRAY_CONFIG_ALL:-$KUBESPRAY_CONFIG_ALL_DEFAULT}"
KUBESPRAY_CONFIG_TEMPLATE="${KUBESPRAY_CONFIG_TEMPLATE:-$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT}"
KUBESPRAY_IMAGE="${KUBESPRAY_IMAGE:-$KUBESPRAY_IMAGE_DEFAULT}"
KUBESPRAY_NODES="${KUBESPRAY_NODES:-$KUBESPRAY_NODES_DEFAULT}"
REUSE_DATA="${REUSE_DATA:-$REUSE_DATA_DEFAULT}"
REUSE_NODES="${REUSE_NODES:-$REUSE_NODES_DEFAULT}"
SNAPSHOT_GIT_BRANCH="${SNAPSHOT_GIT_BRANCH:-$SNAPSHOT_GIT_BRANCH_DEFAULT}"
SNAPSHOT_GIT_REPOSITORY="${SNAPSHOT_GIT_REPOSITORY:-$SNAPSHOT_GIT_REPOSITORY_DEFAULT}"
SNAPSHOT_GIT_USER_EMAIL="${SNAPSHOT_GIT_USER_EMAIL:-$SNAPSHOT_GIT_USER_EMAIL_DEFAULT}"
SNAPSHOT_GIT_USER_NAME="${SNAPSHOT_GIT_USER_NAME:-$SNAPSHOT_GIT_USER_NAME_DEFAULT}"
SSH_KEYFILE="${SSH_KEYFILE:-$SSH_KEYFILE_DEFAULT}"

# Apply templates
BAREMETAL_CSI_INSTALLER_IMAGE="$(
    echo $BAREMETAL_CSI_INSTALLER_IMAGE_TEMPLATE |
        sed "s/__BAREMETAL_CSI__/$BAREMETAL_CSI/g"
)"
BAREMETAL_GPU_NVIDIA_INSTALLER_IMAGE="$(
    echo $BAREMETAL_GPU_INSTALLER_IMAGE_TEMPLATE |
        sed "s/__BAREMETAL_GPU__/nvidia/g"
)"

###########################################################
#   Configure Host                                        #
###########################################################

function configure_linux_kernel() {
    # Disable swap
    sudo swapoff -a
}

# Generate a SSH keypair
function generate_ssh_keypair() {
    if [ ! -f "${SSH_KEYFILE_DEFAULT}" ]; then
        echo "- Generating a SSH Keypair ... "
        mkdir -p "$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT"
        ssh-keygen -q -t rsa -f "$SSH_KEYFILE" -N ''
    fi
}

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
            sudo rm -rf "$KUBERNETES_DATA" || true
            sudo mkdir -p "$KUBERNETES_DATA"
        fi

        # Create a sysctl conf directory if not exists
        sudo mkdir -p "/etc/sysctl.d/"

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
            --tmpfs "/tmp" \
            --volume "/etc/sysctl.d:/etc/sysctl.d" \
            --volume "/lib/modules:/lib/modules" \
            --volume "/sys/fs/bpf:/sys/fs/bpf" \
            --volume "/sys/fs/cgroup:/sys/fs/cgroup" \
            --volume "/sys/kernel/debug:/sys/kernel/debug" \
            --volume "$KUBERNETES_DATA/binary.cni:/opt/cni:shared" \
            --volume "$KUBERNETES_DATA/binary.common:/usr/local/bin:shared" \
            --volume "$KUBERNETES_DATA/binary.etcd:/opt/etcd:shared" \
            --volume "$KUBERNETES_DATA/binary.pypy3:/opt/pypy3:shared" \
            --volume "$KUBERNETES_DATA/etc.cni:/etc/cni:shared" \
            --volume "$KUBERNETES_DATA/etc.containerd:/etc/containerd:shared" \
            --volume "$KUBERNETES_DATA/etc.etcd:/etc/etcd:shared" \
            --volume "$KUBERNETES_DATA/etc.k8s:/etc/kubernetes:shared" \
            --volume "$KUBERNETES_DATA/home.k8s:/root/.kube:shared" \
            --volume "$KUBERNETES_DATA/var.calico:/var/lib/calico:shared" \
            --volume "$KUBERNETES_DATA/var.cni:/var/lib/cni:shared" \
            --volume "$KUBERNETES_DATA/var.containerd:/var/lib/containerd:shared" \
            --volume "$KUBERNETES_DATA/var.k8s:/var/lib/kubelet:shared" \
            --volume "$KUBERNETES_DATA/var.proxy_cache:/var/lib/proxy_cache:shared" \
            --volume "$KUBERNETES_DATA/var.rook:/var/lib/rook:shared" \
            --volume "$KUBERNETES_DATA/var.system.log:/var/log:shared" \
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
        "$CONTAINER_RUNTIME" exec "$name" ps -s 1 |
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
            kubectl get nodes --no-headers "$node_first" \
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

        # Load kiss configurations
        echo -n "- Loading kiss configurations ... "
        "$CONTAINER_RUNTIME" run --rm \
            --name "kiss-configuration-loader" \
            --entrypoint "/usr/bin/env" \
            "$KISS_INSTALLER_IMAGE" \
            tar -cf - -C "./tasks/join/" "." |
            tar -xf - -C "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/tasks/"
        echo "OK"

        # Install cluster
        echo "- Installing k8s cluster ... "
        "$CONTAINER_RUNTIME" run --rm \
            --name "k8s-installer" \
            --net "host" \
            --env "KUBESPRAY_NODES=$nodes" \
            --volume "$KUBESPRAY_CONFIG:/root/kiss/bootstrap/config.yaml:ro" \
            --volume "$KUBESPRAY_CONFIG_ALL:/root/kiss/bootstrap/all.yaml:ro" \
            --volume "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/:/etc/kiss/bootstrap/:ro" \
            --volume "$SSH_KEYFILE:/root/.ssh/id_rsa:ro" \
            --volume "$SSH_KEYFILE.pub:/root/.ssh/id_rsa.pub:ro" \
            "$KUBESPRAY_IMAGE" ansible-playbook \
            --become --become-user="root" \
            --inventory "/etc/kiss/bootstrap/defaults/all.yaml" \
            --inventory "/root/kiss/bootstrap/all.yaml" \
            --inventory "/root/kiss/bootstrap/config.yaml" \
            "/etc/kiss/bootstrap/roles/install-k8s.yaml"

        # Upload kubespray config into nodes
        for name in $KUBESPRAY_NODES; do
            "$CONTAINER_RUNTIME" exec "$node_first" \
                mkdir -p "/root/kiss/bootstrap/"
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                tee "/root/kiss/bootstrap/all.yaml" \
                <"$KUBESPRAY_CONFIG_ALL" |
                echo -n ''
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
            "--from-file=defaults.yaml=/etc/kiss/bootstrap/defaults/all.yaml" \
            "--from-file=hosts.yaml=/etc/kiss/bootstrap/inventory/hosts.yaml" \
            "--from-file=all.yaml=/root/kiss/bootstrap/all.yaml" \
            "--from-file=config.yaml=/root/kiss/bootstrap/config.yaml"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "ansible-config" \
            "--from-literal=kubespray_image=$KUBESPRAY_IMAGE"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss configmap "baremetal-config" \
            "--from-literal=csi=$BAREMETAL_CSI"

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
            "--from-literal=branch=$SNAPSHOT_GIT_BRANCH" \
            "--from-literal=repository=$SNAPSHOT_GIT_REPOSITORY" \
            "--from-literal=user.email=$SNAPSHOT_GIT_USER_EMAIL" \
            "--from-literal=user.name=$SNAPSHOT_GIT_USER_NAME"
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get -n kiss secret "matchbox-account" \
            -o jsonpath='{.data.id_rsa}' |
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                base64 --decode |
            "$CONTAINER_RUNTIME" exec -i "$node_first" \
                tee "/tmp/kiss_snapshot_id_rsa" >/dev/null
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl create -n kiss secret generic "snapshot-git" \
            "--from-file=id_rsa=/tmp/kiss_snapshot_id_rsa" ||
            "$CONTAINER_RUNTIME" exec "$node_first" \
                rm -f "/tmp/kiss_snapshot_id_rsa"
    fi

    # Finished!
    echo "OK"
}

###########################################################
#   Install CSI                                           #
###########################################################

# Define a CSI installer function
function install_csi() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Check if CSI already exists
    local NEED_INSTALL=1
    if [ "$BAREMETAL_CSI" == "none" ]; then
        echo -n "- Skipping installing CSI ... "
        local NEED_INSTALL=0
    elif
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get namespaces "$BAREMETAL_CSI" \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed $BAREMETAL_CSI CSI ... "
        local NEED_INSTALL=0
    fi

    if [ "$NEED_INSTALL" -eq 1 ]; then
        # Install CSI
        echo -n "- Installing $BAREMETAL_CSI CSI in background ... "
        "$CONTAINER_RUNTIME" run --detach --rm \
            --name "csi-installer-$BAREMETAL_CSI" \
            --net "host" \
            --volume "$KUBERNETES_CONFIG:/root/.kube:ro" \
            "$BAREMETAL_CSI_INSTALLER_IMAGE" \
            >/dev/null
    fi

    # Finished!
    echo "OK"
}

###########################################################
#   Install GPU Operator                                 #
###########################################################

# Define a GPU operator installer function
function install_gpu() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Configure environment variables
    local baremetal_gpu="$3"
    local baremetal_gpu_installer_image="$4"
    local baremetal_gpu_name="$2"
    local baremetal_gpu_namespace="gpu-${baremetal_gpu_name}"

    # Check if GPU operator already exists
    local NEED_INSTALL=1
    if [ "$baremetal_gpu" != "true" ]; then
        echo -n "- Skipping installing GPU operator ... "
        local NEED_INSTALL=0
    elif
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get namespaces "$baremetal_gpu_namespace" \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed $baremetal_gpu_name GPU operator ... "
        local NEED_INSTALL=0
    fi

    if [ "$NEED_INSTALL" -eq 1 ]; then
        # Install GPU operator
        echo -n "- Installing $baremetal_gpu_name GPU operator in background ... "
        "$CONTAINER_RUNTIME" run --detach --rm \
            --name "gpu-operator-installer-$baremetal_gpu_name" \
            --net "host" \
            --volume "$KUBERNETES_CONFIG:/root/.kube:ro" \
            "$baremetal_gpu_installer_image" \
            >/dev/null
    fi

    # Finished!
    echo "OK"
}

# Define GPU operators installer function
function install_gpu_all() {
    install_gpu "$1" "nvidia" "$BAREMETAL_GPU_NVIDIA" "$BAREMETAL_GPU_NVIDIA_INSTALLER_IMAGE"
}

###########################################################
#   Install IPIS                                          #
###########################################################

# Define an IPIS cluster installer function
function install_ipis_cluster() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Check if IPIS cluster already exists
    local NEED_INSTALL=1
    if [ "$IPIS_ENABLE" != "true" ]; then
        echo -n "- Skipping installing IPIS cluster ... "
        local NEED_INSTALL=0
    elif
        "$CONTAINER_RUNTIME" exec "$node_first" \
            kubectl get namespaces "ipis" \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed IPIS cluster ... "
        local NEED_INSTALL=0
    fi

    if [ "$NEED_INSTALL" -eq 1 ]; then
        # Install IPIS cluster
        echo -n "- Installing IPIS cluster in background ... "
        "$CONTAINER_RUNTIME" run --detach --rm \
            --name "ipis-installer" \
            --net "host" \
            --volume "$KUBERNETES_CONFIG:/root/.kube:ro" \
            "$IPIS_INSTALLER_IMAGE" \
            >/dev/null
    fi

    # Finished!
    echo "OK"
}

# Define a main function
function main() {
    # Check Dependencies
    check_linux_dependencies

    # Configure Host
    configure_linux_kernel
    generate_ssh_keypair

    # Spawn k8s cluster nodes
    export nodes # results
    for name in $KUBESPRAY_NODES; do
        spawn_node "$name"
    done

    # Install a k8s cluster within nodes
    install_k8s_cluster $KUBESPRAY_NODES

    # Install a kiss cluster within k8s cluster
    install_kiss_cluster $KUBESPRAY_NODES

    # Install a k8s snapshot config within k8s cluster
    install_k8s_snapshot_cluster $KUBESPRAY_NODES

    # Install a CSI
    install_csi $KUBESPRAY_NODES

    # Install GPU operators
    install_gpu_all $KUBESPRAY_NODES

    # Install an IPIS cluster
    install_ipis_cluster $KUBESPRAY_NODES

    # Finished!
    echo "Installed!"
}

# Execute main function
main "$@" || exit 1
