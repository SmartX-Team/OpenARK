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
KISS_CONFIG_PATH_DEFAULT="$(pwd)/config/kiss-config.yaml"
KISS_CONFIG_URL_DEFAULT="https://raw.githubusercontent.com/ulagbulag-village/netai-cloud/master/templates/bootstrap/kiss-config.yaml"
YQ_IMAGE_DEFAULT="docker.io/mikefarah/yq:latest"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
KISS_CONFIG_PATH="${KISS_CONFIG_PATH:-$KISS_CONFIG_PATH_DEFAULT}"
KISS_CONFIG_URL="${KISS_CONFIG_URL:-$KISS_CONFIG_URL_DEFAULT}"
YQ_IMAGE="${YQ_IMAGE:-$YQ_IMAGE_DEFAULT}"

###########################################################
#   Define Configuration Parser                           #
###########################################################

function __kiss_parse() {
    local kind="$1"
    local data="$2"
    local key="$3"
    local var_key="__kiss_${kind}_${key}"

    # try to parse from cache
    if [ -z "${!var_key+x}" ]; then
        # resolve data
        declare $var_key=$(cat "${KISS_CONFIG_PATH}" |
            "${CONTAINER_RUNTIME}" run --interactive --rm "${YQ_IMAGE}" \
                ". | select(.kind == \"${kind}\") | .${data}.${key}")
    fi
    echo "${!var_key}"
}

function kiss_validate_config_file() {
    if [ ! -f "${KISS_CONFIG_PATH}" ]; then
        echo "- Downloading default KISS configuration file to \"${KISS_CONFIG_PATH}\"..."
        mkdir -p "$(dirname "${KISS_CONFIG_PATH}")"
        curl -o "${KISS_CONFIG_PATH}" "${KISS_CONFIG_URL}"
    fi

    # Define dynamic environment variables
    export KUBESPRAY_NODES="$(kiss_config 'bootstrapper_node_name') "
}

function kiss_config() {
    local key="$1"
    __kiss_parse "ConfigMap" "data" "${key}"
}

function kiss_secret() {
    local key="$1"
    __kiss_parse "Secret" "stringData" "${key}"
}

###########################################################
#   Configure Host                                        #
###########################################################

function configure_linux_kernel() {
    # Disable swap
    sudo swapoff -a
}

# Generate a SSH keypair
function generate_ssh_keypair() {
    local key_file="$(kiss_config 'bootstrapper_auth_ssh_key_path')"
    if [ ! -f "${key_file}" ]; then
        echo "- Generating a SSH Keypair ... "
        mkdir -p "$(dirname ${key_file})"
        ssh-keygen -q -t ed25519 -f "${key_file}" -N ''
    fi
}

###########################################################
#   Spawn nodes                                           #
###########################################################

# Define a node spawner function
function spawn_node() {
    local name="$1"

    # Parse variables
    local KISS_BOOTSTRAP_NODE_IMAGE="$(kiss_config 'bootstrapper_node_image')"
    local KUBERNETES_DATA="$(kiss_config 'bootstrapper_node_data_kubernetes_path')"
    local REUSE_KUBERNETES_DATA="$(kiss_config 'bootstrapper_node_reuse_data_kubernetes')"
    local REUSE_NODES="$(kiss_config 'bootstrapper_node_reuse_container')"
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"

    # Check if node already exists
    local NEED_SPAWN=1
    if [ $("${CONTAINER_RUNTIME}" ps -a -q -f "name=${name}") ]; then
        if [ $(echo "${REUSE_NODES}" | awk '{print tolower($0)}') == "true" ]; then
            echo -n "- Using already spawned node (${name}) ... "
            local NEED_SPAWN=0
        else
            echo "Error: Already spawned node (${name})"
            exit 1
        fi
    fi

    if [ "${NEED_SPAWN}" -eq 1 ]; then
        # Reset data
        if [ $(echo "${REUSE_KUBERNETES_DATA}" | awk '{print tolower($0)}') == "false" ]; then
            echo "- Removing previous data ... "
            sudo rm -rf "${KUBERNETES_DATA}" || true
        fi
        sudo mkdir -p "${KUBERNETES_DATA}"
        local KUBERNETES_DATA="$(realpath "${KUBERNETES_DATA}")"

        # Create a sysctl conf directory if not exists
        sudo mkdir -p "/etc/sysctl.d/"

        # Spawn a node
        echo -n "- Spawning a node (${name}) ... "
        "$CONTAINER_RUNTIME" run --detach \
            --name "${name}" \
            --cgroupns "host" \
            --hostname "${name}" \
            --ipc "host" \
            --net "host" \
            --privileged \
            --env "SSH_PUBKEY=$(cat ${SSH_KEYFILE}.pub)" \
            --log-opt "max-size=100m" \
            --log-opt "max-file=5" \
            --restart "unless-stopped" \
            --tmpfs "/run" \
            --tmpfs "/tmp" \
            --volume "/etc/sysctl.d:/etc/sysctl.d" \
            --volume "/lib/modules:/lib/modules" \
            --volume "/sys/fs/bpf:/sys/fs/bpf" \
            --volume "/sys/fs/cgroup:/sys/fs/cgroup" \
            --volume "/sys/kernel/debug:/sys/kernel/debug" \
            --volume "${KUBERNETES_DATA}/binary.cni:/opt/cni:shared" \
            --volume "${KUBERNETES_DATA}/binary.common:/usr/local/bin:shared" \
            --volume "${KUBERNETES_DATA}/binary.etcd:/opt/etcd:shared" \
            --volume "${KUBERNETES_DATA}/binary.pypy3:/opt/pypy3:shared" \
            --volume "${KUBERNETES_DATA}/etc.cni:/etc/cni:shared" \
            --volume "${KUBERNETES_DATA}/etc.containerd:/etc/containerd:shared" \
            --volume "${KUBERNETES_DATA}/etc.etcd:/etc/etcd:shared" \
            --volume "${KUBERNETES_DATA}/etc.k8s:/etc/kubernetes:shared" \
            --volume "${KUBERNETES_DATA}/home.k8s:/root/.kube:shared" \
            --volume "${KUBERNETES_DATA}/var.calico:/var/lib/calico:shared" \
            --volume "${KUBERNETES_DATA}/var.cni:/var/lib/cni:shared" \
            --volume "${KUBERNETES_DATA}/var.containerd:/var/lib/containerd:shared" \
            --volume "${KUBERNETES_DATA}/var.dnsmasq:/var/lib/dnsmasq:shared" \
            --volume "${KUBERNETES_DATA}/var.k8s:/var/lib/kubelet:shared" \
            --volume "${KUBERNETES_DATA}/var.proxy_cache:/var/lib/proxy_cache:shared" \
            --volume "${KUBERNETES_DATA}/var.rook:/var/lib/rook:shared" \
            --volume "${KUBERNETES_DATA}/var.system.log:/var/log:shared" \
            "${KISS_BOOTSTRAP_NODE_IMAGE}" >/dev/null
    else
        # Start SSH
        "${CONTAINER_RUNTIME}" exec "${name}" systemctl start sshd
    fi

    # Get suitable access IP
    node_ip=$(
        "${CONTAINER_RUNTIME}" exec "${name}" ip a |
            grep -o '10\.\(3[2-9]\|4[0-7]\)\(\.\(25[0-5]\|\(2[0-4]\|1[0-9]\|[1-9]\|\)[0-9]\)\)\{2\}' |
            head -1
    )
    if [ ! "${node_ip}" ]; then
        echo "Err"
        echo "Error: Cannot find host IP (10.32.0.0/12)"
        exit 1
    fi

    # Update SSH ListenAddress
    "${CONTAINER_RUNTIME}" exec "${name}" sed -i \
        "s/^\(ListenAddress\) .*\$/\1 ${node_ip}/g" \
        "/etc/ssh/sshd_config"

    # Restart SSH daemon
    while [ ! $(
        "${CONTAINER_RUNTIME}" exec "${name}" ps -s 1 |
            awk '{print $4}' |
            tail -n 1 |
            grep '^systemd'
    ) ]; do
        sleep 1
    done
    "${CONTAINER_RUNTIME}" exec "${name}" \
        systemctl restart sshd 2>/dev/null || true

    # Get SSH configuration
    while :; do
        # Get SSH port
        local SSH_PORT="$(
            "${CONTAINER_RUNTIME}" exec "${name}" cat /etc/ssh/sshd_config |
                grep '^Port ' |
                awk '{print $2}'
        )"

        # Try connect to the node
        if
            ssh \
                -o "StrictHostKeyChecking=no" \
                -o "UserKnownHostsFile=/dev/null" \
                -p "${SSH_PORT}" \
                -i "${SSH_KEYFILE}" \
                "root@${node_ip}" \
                exit \
                2>/dev/null
        then
            break
        fi
    done

    # Save as environment variable
    local node="${name}:${node_ip}:${SSH_PORT}"
    export nodes="${nodes} ${node}"

    # Finished!
    echo "OK (${node})"
}

###########################################################
#   Install k8s cluster                                   #
###########################################################

# Define a k8s cluster installer function
function install_k8s_cluster() {
    local names="$1"
    local node_first="$(echo ${names} | awk '{print $1}')"

    # Parse variables
    local KISS_INSTALLER_IMAGE="$(kiss_config 'kiss_installer_image')"
    local KUBERNETES_CONFIG="$(realpath $(eval echo $(kiss_config 'bootstrapper_kubernetes_config_path')))"
    local KUBESPRAY_CONFIG="$(kiss_config 'bootstrapper_kubespray_config_path')"
    local KUBESPRAY_CONFIG_ALL="$(kiss_config 'bootstrapper_kubespray_config_all_path')"
    local KUBESPRAY_CONFIG_TEMPLATE="$(kiss_config 'bootstrapper_kubespray_config_template_path')"
    local KUBESPRAY_IMAGE="$(kiss_config 'kubespray_image')"
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"

    # Check if k8s cluster already exists
    local NEED_INSTALL=1
    if
        "${CONTAINER_RUNTIME}" exec "${node_first}" \
            kubectl get nodes --no-headers "${node_first}" \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed k8s cluster ... "
        local NEED_INSTALL=0
    fi

    if [ "${NEED_INSTALL}" -eq 1 ]; then
        # Cleanup
        rm -rf "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"

        # Get a sample kubespray configuration file
        mkdir -p "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"
        "${CONTAINER_RUNTIME}" exec "${node_first}" \
            tar -cf - -C "/etc/kiss/bootstrap/" "." |
            tar -xf - -C "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"

        # Load kiss configurations
        echo -n "- Loading kiss configurations ... "
        "${CONTAINER_RUNTIME}" run --rm \
            --name "kiss-configuration-loader" \
            --entrypoint "/usr/bin/env" \
            "${KISS_INSTALLER_IMAGE}" \
            tar -cf - -C "./tasks/common/" "." |
            tar -xf - -C "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/tasks/"

        # Update paths
        local KUBESPRAY_CONFIG="$(realpath "${KUBESPRAY_CONFIG}")"
        local KUBESPRAY_CONFIG_ALL="$(realpath "${KUBESPRAY_CONFIG_ALL}")"
        local KUBESPRAY_CONFIG_TEMPLATE="$(realpath "${KUBESPRAY_CONFIG_TEMPLATE}")"
        echo "OK"

        # Remove last cluster if exists
        echo "- Resetting last k8s cluster ... "
        "${CONTAINER_RUNTIME}" run --rm \
            --name "k8s-reset" \
            --net "host" \
            --env "KUBESPRAY_NODES=${nodes}" \
            --volume "${KUBESPRAY_CONFIG}:/root/kiss/bootstrap/config.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_ALL}:/root/kiss/bootstrap/all.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/:/etc/kiss/bootstrap/:ro" \
            --volume "${SSH_KEYFILE}:/root/.ssh/id_ed25519:ro" \
            --volume "${SSH_KEYFILE}.pub:/root/.ssh/id_ed25519.pub:ro" \
            "${KUBESPRAY_IMAGE}" ansible-playbook \
            --become --become-user="root" \
            --inventory "/etc/kiss/bootstrap/defaults/all.yaml" \
            --inventory "/root/kiss/bootstrap/all.yaml" \
            --inventory "/root/kiss/bootstrap/config.yaml" \
            "/etc/kiss/bootstrap/roles/reset-k8s.yaml" || true

        # Install cluster
        echo "- Installing k8s cluster ... "
        "${CONTAINER_RUNTIME}" run --rm \
            --name "k8s-installer" \
            --net "host" \
            --env "KUBESPRAY_NODES=${nodes}" \
            --volume "${KUBESPRAY_CONFIG}:/root/kiss/bootstrap/config.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_ALL}:/root/kiss/bootstrap/all.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/:/etc/kiss/bootstrap/:ro" \
            --volume "${SSH_KEYFILE}:/root/.ssh/id_ed25519:ro" \
            --volume "${SSH_KEYFILE}.pub:/root/.ssh/id_ed25519.pub:ro" \
            "${KUBESPRAY_IMAGE}" ansible-playbook \
            --become --become-user="root" \
            --inventory "/etc/kiss/bootstrap/defaults/all.yaml" \
            --inventory "/root/kiss/bootstrap/all.yaml" \
            --inventory "/root/kiss/bootstrap/config.yaml" \
            "/etc/kiss/bootstrap/roles/install-k8s.yaml"

        # Upload kubespray config into nodes
        for name in ${KUBESPRAY_NODES}; do
            "${CONTAINER_RUNTIME}" exec "${node_first}" \
                mkdir -p "/root/kiss/bootstrap/"
            "${CONTAINER_RUNTIME}" exec -i "${node_first}" \
                tee "/root/kiss/bootstrap/all.yaml" \
                <"${KUBESPRAY_CONFIG_ALL}" |
                echo -n ''
            "${CONTAINER_RUNTIME}" exec -i "${node_first}" \
                tee "/root/kiss/bootstrap/config.yaml" \
                <"${KUBESPRAY_CONFIG}" |
                echo -n ''
        done

        # Download k8s config into host
        mkdir -p "${KUBERNETES_CONFIG}"
        "${CONTAINER_RUNTIME}" exec "${node_first}" \
            tar -cf - -C "/root/.kube" "." |
            tar -xf - -C "${KUBERNETES_CONFIG}"

        # Cleanup
        rm -rf "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"
    fi

    # Finished!
    echo "OK"
}

###########################################################
#   Install KISS Cluster                                  #
###########################################################

# Define a KISS cluster installer function
function install_kiss_cluster() {
    local names="$1"
    local node_first="$(echo ${names} | awk '{print $1}')"

    # Parse variables
    local KISS_INSTALLER_IMAGE="$(kiss_config 'kiss_installer_image')"
    local KUBERNETES_CONFIG="$(realpath $(eval echo $(kiss_config 'bootstrapper_kubernetes_config_path')))"
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"

    # Check if kiss cluster already exists
    local NEED_INSTALL=1
    if
        "${CONTAINER_RUNTIME}" exec "${node_first}" \
            kubectl get namespaces kiss \
            >/dev/null 2>/dev/null
    then
        echo -n "- Using already installed kiss cluster ... "
        local NEED_INSTALL=0
    fi

    if [ "${NEED_INSTALL}" -eq 1 ]; then
        # Upload the K8S Configuration File to the Cluster
        "${CONTAINER_RUNTIME}" exec "${node_first}" \
            kubectl create namespace kiss
        cat "${KISS_CONFIG_PATH}" |
            "${CONTAINER_RUNTIME}" run --interactive --rm "${YQ_IMAGE}" \
                "(select(.kind == \"ConfigMap\") | .data.auth_ssh_key_id_ed25519_public) = \"$(
                    cat "${SSH_KEYFILE}.pub" |
                        awk '{print $1 " " $2}'
                )\"" |
            "${CONTAINER_RUNTIME}" run --interactive --rm "${YQ_IMAGE}" \
                "(select(.kind == \"ConfigMap\") | .data.kiss_cluster_name) = \"default\"" |
            "${CONTAINER_RUNTIME}" run --interactive --rm "${YQ_IMAGE}" \
                "(select(.kind == \"Secret\") | .stringData.auth_ssh_key_id_ed25519) = \"$(
                    cat "${SSH_KEYFILE}"
                )\n\"" |
            "${CONTAINER_RUNTIME}" exec -i "${node_first}" \
                kubectl apply -f -
        "${CONTAINER_RUNTIME}" exec "${node_first}" \
            kubectl create -n kiss configmap "ansible-control-planes-default" \
            "--from-file=defaults.yaml=/etc/kiss/bootstrap/defaults/all.yaml" \
            "--from-file=hosts.yaml=/etc/kiss/bootstrap/inventory/hosts.yaml" \
            "--from-file=all.yaml=/root/kiss/bootstrap/all.yaml" \
            "--from-file=config.yaml=/root/kiss/bootstrap/config.yaml"

        # Install cluster
        echo "- Installing kiss cluster ... "
        "${CONTAINER_RUNTIME}" run --rm \
            --name "kiss-installer" \
            --net "host" \
            --volume "${KUBERNETES_CONFIG}:/root/.kube:ro" \
            "${KISS_INSTALLER_IMAGE}"

        # Show how to deploy your SSH keys into the Web (i.e. Github) repository.
        echo
        echo "* NOTE: You can register the SSH public key to activate the snapshot manager."
        echo "* Your SSH key: \"$(
            cat "${SSH_KEYFILE}.pub" |
                awk '{print $1 " " $2}'
        )\""
        echo "* Your SSH key is saved on: \"${SSH_KEYFILE}.pub\""
        echo "* Learn How to store keys (Github): \"https://docs.github.com/en/developers/overview/managing-deploy-keys#deploy-keys\""
        echo
    fi

    # Finished!
    echo "OK"
}

###########################################################
#   Main Function                                         #
###########################################################

# Define a main function
function main() {
    # Validate Configurations
    kiss_validate_config_file

    # Configure Host
    configure_linux_kernel
    generate_ssh_keypair

    # Spawn k8s cluster nodes
    export nodes # results
    for name in ${KUBESPRAY_NODES}; do
        spawn_node "${name}"
    done

    # Install a k8s cluster within nodes
    install_k8s_cluster ${KUBESPRAY_NODES}

    # Install a KISS cluster within k8s cluster
    install_kiss_cluster ${KUBESPRAY_NODES}

    # Finished!
    echo "Installed!"
}

# Execute main function
main "$@" || exit 1
