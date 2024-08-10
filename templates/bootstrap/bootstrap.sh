#!/bin/bash
# Copyright (c) 2022-2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
CONTAINER_RUNTIME_DEFAULT="docker"
INSTALLER_TYPE_DEFAULT="host" # One of: container, host (default), iso
IPCALC_IMAGE_DEFAULT="docker.io/debber/ipcalc:latest"
ISO_BASE_URL_DEFAULT="https://download.rockylinux.org/pub/rocky/9/BaseOS/$(uname -m)/os/images/boot.iso"
KISS_BOOTSTRAPPER_URL_DEFAULT="https://raw.githubusercontent.com/ulagbulag/openark/master/templates/bootstrap/bootstrap.sh"
KISS_CONFIG_PATH_DEFAULT="$(pwd)/config/kiss-config.yaml"
KISS_CONFIG_URL_DEFAULT="https://raw.githubusercontent.com/ulagbulag/openark/master/templates/bootstrap/kiss-config.yaml"
XORRISO_IMAGE_DEFAULT="docker.io/codeocean/xorriso:latest"
YQ_IMAGE_DEFAULT="docker.io/mikefarah/yq:latest"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
INSTALLER_TYPE="${INSTALLER_TYPE:-$INSTALLER_TYPE_DEFAULT}"
IPCALC_IMAGE="${IPCALC_IMAGE:-$IPCALC_IMAGE_DEFAULT}"
ISO_BASE_URL="${ISO_BASE_URL:-$ISO_BASE_URL_DEFAULT}"
KISS_BOOTSTRAPPER_URL="${KISS_BOOTSTRAPPER_URL:-$KISS_BOOTSTRAPPER_URL_DEFAULT}"
KISS_CONFIG_PATH="${KISS_CONFIG_PATH:-$KISS_CONFIG_PATH_DEFAULT}"
KISS_CONFIG_URL="${KISS_CONFIG_URL:-$KISS_CONFIG_URL_DEFAULT}"
XORRISO_IMAGE="${XORRISO_IMAGE:-$XORRISO_IMAGE_DEFAULT}"
YQ_IMAGE="${YQ_IMAGE:-$YQ_IMAGE_DEFAULT}"

###########################################################
#   Define Console Logger                                 #
###########################################################

function __log() {
    local level=$1
    local content=$2

    local reset='\033[0m'
    case "x${level}" in
    'xPATCH')
        local color='\033[35m'
        local important=0
        ;;
    'xSKIP')
        local color='\033[33m'
        local important=0
        ;;
    'xINFO')
        local color='\033[1;92m'
        local important=1
        ;;
    'xDONE')
        local color='\033[1;94m'
        local important=0
        ;;
    'xWARN')
        local color='\033[1;93m'
        local important=1
        ;;
    'xERROR')
        local color='\033[1;91m'
        local important=1
        ;;
    *)
        local color="${reset}"
        local important=0
        ;;
    esac

    local msg="${color} - ${content}${reset}\n"
    if [ "x${important}" = 'x1' ]; then
        local divider='================================================================================'
        local msg="${divider}\n${msg}"
    fi

    if [ "x${level}" = 'xERROR' ]; then
        printf "${msg}" >&2
        exit 1
    else
        printf "${msg}"
    fi
}

###########################################################
#   Define Dependency Checker                             #
###########################################################

function check_dependencies() {
    # Switch to podman on host install
    if [ "x${INSTALLER_TYPE}" = 'xhost' ]; then
        __log 'PATCH' "Switching default container runtime to podman"
        CONTAINER_RUNTIME='podman'
    fi

    # Check container runtime and install if not exists
    if ! which ${CONTAINER_RUNTIME} >/dev/null; then
        __log 'WARN' "WARN: Cannot find container runtime \"${CONTAINER_RUNTIME}\"" >&2

        case "x${CONTAINER_RUNTIME}" in
        'xdocker')
            __log 'PATCH' "Installing \"${CONTAINER_RUNTIME}\"..."
            curl --proto '=https' -sSf 'https://get.docker.com' | sudo sh
            ;;
        *)
            exit 1
            ;;
        esac
    fi

    # Check container runtime is running
    if ! which ${CONTAINER_RUNTIME} >/dev/null; then
        exit 1
    fi
    if [ "x${CONTAINER_RUNTIME}" = 'xdocker' ]; then
        sudo systemctl enable --now ${CONTAINER_RUNTIME}
        sudo systemctl enable --now "${CONTAINER_RUNTIME}.socket"
    fi

    # Check user has permission to access to container runtime
    if ! ${CONTAINER_RUNTIME} version >/dev/null 2>/dev/null; then
        export CONTAINER_RUNTIME="sudo ${CONTAINER_RUNTIME}"
    fi
}

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
        declare $var_key="$(
            cat "${KISS_CONFIG_PATH}" |
                ${CONTAINER_RUNTIME} run --interactive --rm "${YQ_IMAGE}" \
                    "select(.kind == \"${kind}\") | .${data}.${key}"
        )"
    fi
    echo "${!var_key}"
}

function __kiss_patch() {
    local kind="$1"
    local data="$2"
    local key="$3"
    local value="$4"

    # patched file
    local patched_file="${KISS_CONFIG_PATH}.patched"

    # patch data
    __log 'PATCH' "Patching ${kind}: ${key}"
    cat "${KISS_CONFIG_PATH}" |
        ${CONTAINER_RUNTIME} run --interactive --rm "${YQ_IMAGE}" \
            "(select(.kind == \"${kind}\") | .${data}.${key}) = ${value}" \
            >"${patched_file}"
    mv "${patched_file}" "${KISS_CONFIG_PATH}"
}

function kiss_validate_config_file() {
    if [ ! -f "${KISS_CONFIG_PATH}" ]; then
        __log 'PATCH' "Downloading default KISS configuration file to \"${KISS_CONFIG_PATH}\"..."
        mkdir -p "$(dirname "${KISS_CONFIG_PATH}")"
        curl -o "${KISS_CONFIG_PATH}" "${KISS_CONFIG_URL}"

        __log 'ERROR' "NOTE: Please configure the file and try it agait; Aborting."
    fi

    # Set default cluster name
    kiss_patch_config 'kiss_cluster_name' "\"default\""

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

function kiss_patch_config() {
    local key="$1"
    local value="$2"
    __kiss_patch "ConfigMap" "data" "${key}" "${value}"
}

function kiss_patch_secret() {
    local key="$1"
    local value="$2"
    __kiss_patch "Secret" "stringData" "${key}" "${value}"
}

###########################################################
#   Configure Host                                        #
###########################################################

function configure_env() {
    # Update PATH
    export PATH="${PATH}:/usr/local/bin"
}

function configure_linux_kernel() {
    # Disable swap
    sudo swapoff -a
}

function find_public_ip() {
    prefix=$1

    # Get suitable access IP
    netdev="$(
        ${prefix} ip route show |
            grep -Po '^default via [0-9.]+ dev \K[a-z0-9]+' |
            head -n1
    )"
    if [ ! "${netdev}" ]; then
        __log 'ERROR' "Error: Cannot find an active network device"
    fi

    public_ip="$(
        ${prefix} ip addr show dev ${netdev} |
            grep -Po '^ +inet \K[0-9.]+'
    )"
    if [ ! "${public_ip}" ]; then
        __log 'ERROR' "Error: Cannot find a public host IP"
    fi

    echo "${public_ip}"
}

# Generate a SSH keypair
function generate_ssh_keypair() {
    local key_file="$(kiss_config 'bootstrapper_auth_ssh_key_path')"
    if [ ! -f "${key_file}" ]; then
        __log 'INFO' "Generating a SSH Keypair ..."
        mkdir -p "$(dirname ${key_file})"
        ssh-keygen -q -t ed25519 -f "${key_file}" -N ''
    else
        __log 'SKIP' "Skipping generating a SSH Keypair ..."
    fi

    # Patch configs
    kiss_patch_config 'auth_ssh_key_id_ed25519_public' "\"$(
        cat "${key_file}.pub" |
            awk '{print $1 " " $2}'
    )\""
    kiss_patch_secret 'auth_ssh_key_id_ed25519' "\"$(
        cat "${key_file}"
    )\n\""
}

###########################################################
#   Configure users                                       #
###########################################################

# Define a default user creation function
function create_user() {
    prefix=$1

    # Configure user data
    local USER_GID="2000"
    local USER_NAME="$(kiss_config 'auth_ssh_username')"
    local USER_SHELL="bash"
    local USER_UID="2000"

    # Create an user if not exists
    if ! $(${prefix} cat /etc/passwd | grep -Poq "^${USER_NAME}:"); then
        __log 'INFO' "Creating an user: ${USER_NAME}"

        ${prefix} groupadd -g "${USER_GID}" -o "${USER_NAME}" || true
        ${prefix} useradd -u "${USER_UID}" -g "${USER_GID}" \
            -G "audio,cdrom,input,pipewire,render,video" \
            -s "/bin/${USER_SHELL}" -m -o "${USER_NAME}" ||
            true

        # Enable cgroup2 namespace
        ${prefix} cat /etc/subuid | grep -Poq "^${USER_UID}:" ||
            echo -e "${USER_UID}:2001:65535" | ${prefix} tee -a /etc/subuid
        ${prefix} cat /etc/subgid | grep -Poq "^${USER_GID}:" ||
            echo -e "${USER_GID}:2001:65535" | ${prefix} tee -a /etc/subgid
    else
        __log 'SKIP' "Skipping creating an user: ${USER_NAME}"
    fi

    # Finished!
    __log 'DONE' "User is ready: ${USER_NAME}"
}

###########################################################
#   Define shell                                          #
###########################################################

function __shell() {
    local node_first="$1"

    if [ "x${INSTALLER_TYPE}" = 'xhost' ]; then
        echo -n "sudo env PATH='${PATH}'"
    else
        echo -n "${CONTAINER_RUNTIME} exec ${node_first}"
    fi
}

###########################################################
#   Spawn nodes                                           #
###########################################################

# Define a containerized node spawner function
function spawn_node_on_container() {
    local name="$1"

    # Parse variables
    local KISS_BOOTSTRAP_NODE_IMAGE="$(kiss_config 'bootstrapper_node_image')"
    local KUBERNETES_DATA="$(kiss_config 'bootstrapper_node_data_kubernetes_path')"
    local REUSE_KUBERNETES_DATA="$(kiss_config 'bootstrapper_node_reuse_data_kubernetes')"
    local REUSE_NODES="$(kiss_config 'bootstrapper_node_reuse_container')"
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"

    # Check if node already exists
    local NEED_SPAWN=1
    if [ $(${CONTAINER_RUNTIME} ps -a -q -f "name=${name}") ]; then
        if [ $(echo "${REUSE_NODES}" | awk '{print tolower($0)}') == "true" ]; then
            __log 'SKIP' "Using already spawned node (${name}) ..."
            local NEED_SPAWN=0
        else
            __log 'ERROR' "Error: Already spawned node (${name})"
        fi
    fi

    if [ "x${NEED_SPAWN}" == "x1" ]; then
        __log 'INFO' "Preparing for creating a node container ..."

        # Reset data
        if [ $(echo "${REUSE_KUBERNETES_DATA}" | awk '{print tolower($0)}') == "false" ]; then
            __log 'PATCH' "Removing previous data ..."
            sudo rm -rf "${KUBERNETES_DATA}" || true
        fi
        sudo mkdir -p "${KUBERNETES_DATA}"
        local KUBERNETES_DATA="$(realpath "${KUBERNETES_DATA}")"

        # Create a sysctl conf directory if not exists
        sudo mkdir -p "/etc/sysctl.d/"

        # Define data directories
        declare -a DATA_DIRS=(
            "/binary.cni:/opt/cni"
            "/binary.common:/usr/local/bin"
            "/binary.etcd:/opt/etcd"
            "/binary.pypy3:/opt/pypy3"
            "/etc.cni:/etc/cni"
            "/etc.containerd:/etc/containerd"
            "/etc.etcd:/etc/etcd"
            "/etc.k8s:/etc/kubernetes"
            "/home.k8s:/root/.kube"
            "/var.calico:/var/lib/calico"
            "/var.cni:/var/lib/cni"
            "/var.containerd:/var/lib/containerd"
            "/var.dnsmasq:/var/lib/dnsmasq"
            "/var.k8s:/var/lib/kubelet"
            "/var.proxy_cache:/var/lib/proxy_cache"
            "/var.rook:/var/lib/rook"
            "/var.system.log:/var/log"
        )

        # Create data directories
        local CONTAINER_ARGS=""
        for data_dir in ${DATA_DIRS[@]}; do
            data_src="${KUBERNETES_DATA}$(echo "${data_dir}" | cut '-d:' '-f1')"
            data_dst="$(echo "${data_dir}" | cut '-d:' '-f2')"

            sudo mkdir -p "${data_src}"
            CONTAINER_ARGS="${CONTAINER_ARGS} --volume ${data_src}:${data_dst}:shared"
        done

        # Spawn a node
        __log 'INFO' "Spawning a node (${name}) ..."
        ${CONTAINER_RUNTIME} run --detach \
            --name "${name}" \
            --cgroupns "host" \
            --hostname "${name}" \
            --ipc "host" \
            --net "host" \
            --privileged \
            --pull "always" \
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
            ${CONTAINER_ARGS} \
            "${KISS_BOOTSTRAP_NODE_IMAGE}" >/dev/null
    else
        # Start SSH
        ${CONTAINER_RUNTIME} exec "${name}" systemctl start sshd
    fi

    # Create a default user if not exists
    create_user "${CONTAINER_RUNTIME} exec -i ${name}"

    # Get suitable access IP
    local node_ip="$(find_public_ip "${CONTAINER_RUNTIME} exec ${name}")"

    # Update SSH ListenAddress
    ${CONTAINER_RUNTIME} exec "${name}" sed -i \
        "s/^\(ListenAddress\) .*\$/\1 ${node_ip}/g" \
        "/etc/ssh/sshd_config"

    # Restart SSH daemon
    while [ ! $(
        ${CONTAINER_RUNTIME} exec "${name}" ps -s 1 |
            awk '{print $4}' |
            tail -n 1 |
            grep '^systemd'
    ) ]; do
        sleep 1
    done
    ${CONTAINER_RUNTIME} exec "${name}" \
        systemctl restart sshd 2>/dev/null || true

    # Get SSH configuration
    while :; do
        # Get SSH port
        local SSH_PORT="$(
            ${CONTAINER_RUNTIME} exec "${name}" cat /etc/ssh/sshd_config |
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
    __log 'DONE' "Node is ready: ${node}"
}

# Define a host node spawner function
function spawn_node_on_host() {
    local name="$1"

    # Parse variables
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"
    local USER_NAME="$(kiss_config 'auth_ssh_username')"

    __log 'INFO' "Preparing for using the host as a node ..."

    # Create a default user if not exists
    create_user "sudo"

    # Register the ssh keys
    __log 'PATCH' "Registering the ssh keys: ${USER_NAME}"
    sudo cat "/home/${USER_NAME}/.ssh/authorized_keys" | grep -Poq "^$(cat ${SSH_KEYFILE}.pub)$" ||
        echo "$(cat ${SSH_KEYFILE}.pub)" | sudo tee -a "/home/${USER_NAME}/.ssh/authorized_keys" >/dev/null

    # Get suitable access IP
    local node_ip="$(find_public_ip)"
    local SSH_PORT="22"

    # Save as environment variable
    local node="${name}:${node_ip}:${SSH_PORT}"
    export nodes="${nodes} ${node}"

    # Finished!
    __log 'DONE' "Node is ready: ${node}"
}

###########################################################
#   Install k8s cluster                                   #
###########################################################

# Define a k8s cluster installer function
function install_k8s_cluster() {
    local names="$1"
    local node_first="$(echo ${names} | awk '{print $1}')"

    # Parse variables
    local KISS_BOOTSTRAP_NODE_IMAGE="$(kiss_config 'bootstrapper_node_image')"
    local KISS_INSTALLER_IMAGE="$(kiss_config 'kiss_installer_image')"
    local KUBERNETES_CONFIG="$(realpath $(eval echo $(kiss_config 'bootstrapper_kubernetes_config_path')))"
    local KUBESPRAY_CONFIG="$(kiss_config 'bootstrapper_kubespray_config_path')"
    local KUBESPRAY_CONFIG_ALL="$(kiss_config 'bootstrapper_kubespray_config_all_path')"
    local KUBESPRAY_CONFIG_TEMPLATE="$(kiss_config 'bootstrapper_kubespray_config_template_path')"
    local KUBESPRAY_IMAGE="$(kiss_config 'kubespray_image')"
    local REUSE_KUBERNETES="$(kiss_config 'bootstrapper_kubernetes_reuse')"
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"
    local USER_NAME="$(kiss_config 'auth_ssh_username')"

    # Check if k8s cluster already exists
    local NEED_INSTALL=1
    if
        $(__shell "${node_first}") \
            kubectl get nodes --no-headers "${node_first}" \
            >/dev/null 2>/dev/null
    then
        if [ "x${REUSE_KUBERNETES}" = 'xtrue' ]; then
            __log 'SKIP' "Using already installed k8s cluster ..."
            local NEED_INSTALL=0
        fi
    fi

    if [ "x${NEED_INSTALL}" == "x1" ]; then
        __log 'INFO' "Preparing for deploying a kubernetes cluster ..."

        # Cleanup
        __log 'PATCH' "Cleaning up old configurations ..."
        rm -rf "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"

        # Get sample kubespray configurations
        __log 'PATCH' "Loading kubespray configurations ..."
        mkdir -p "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"
        ${CONTAINER_RUNTIME} run \
            --entrypoint /usr/bin/env \
            --log-opt "max-size=100m" \
            --log-opt "max-file=5" \
            --net "host" \
            --pull "always" \
            --rm \
            --tmpfs "/run" \
            --tmpfs "/tmp" \
            "${KISS_BOOTSTRAP_NODE_IMAGE}" \
            tar -cf - -C "/etc/kiss/bootstrap/" "." |
            tar -xf - -C "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"

        # Load kiss configurations
        __log 'PATCH' "Loading kiss configurations ..."
        ${CONTAINER_RUNTIME} run --rm \
            --name "kiss-configuration-loader" \
            --entrypoint "/usr/bin/env" \
            "${KISS_INSTALLER_IMAGE}" \
            tar -cf - -C "./tasks/common/" "." |
            tar -xf - -C "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/tasks/"

        # Update paths
        local KUBESPRAY_CONFIG="$(realpath "${KUBESPRAY_CONFIG}")"
        local KUBESPRAY_CONFIG_ALL="$(realpath "${KUBESPRAY_CONFIG_ALL}")"
        local KUBESPRAY_CONFIG_TEMPLATE="$(realpath "${KUBESPRAY_CONFIG_TEMPLATE}")"
        __log 'DONE' "Loaded configurations"

        # Create a host inventory
        __log 'INFO' "Creating a host inventory ..."
        ${CONTAINER_RUNTIME} run --rm --tty \
            --name "k8s-generate-inventory" \
            --net "host" \
            --env "ansible_user=${USER_NAME}" \
            --env "KUBESPRAY_NODES=${nodes}" \
            --pull "always" \
            --volume "${KUBESPRAY_CONFIG}:/root/kiss/bootstrap/config.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_ALL}:/root/kiss/bootstrap/all.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/:/etc/kiss/bootstrap/:rw" \
            --volume "${SSH_KEYFILE}:/root/.ssh/id_ed25519:ro" \
            --volume "${SSH_KEYFILE}.pub:/root/.ssh/id_ed25519.pub:ro" \
            "${KUBESPRAY_IMAGE}" bash -c '
                sed -i "s/\(^\- name\: reset | Restart network$\)/\1\n  ignore_errors\: true/g" /kubespray/roles/reset/tasks/main.yml \
                && exec ansible-playbook \
                    --become --become-user="root" \
                    --inventory "/root/kiss/bootstrap/all.yaml" \
                    --inventory "/root/kiss/bootstrap/config.yaml" \
                    "/etc/kiss/bootstrap/roles/generate-inventory.yaml"
            '

        # Remove last cluster if exists
        if [ "x${REUSE_KUBERNETES}" = 'xtrue' ]; then
            __log 'SKIP' "Skipping resetting last k8s cluster ..."
        else
            __log 'INFO' "Resetting last k8s cluster ..."
            ${CONTAINER_RUNTIME} run --rm --tty \
                --name "k8s-reset" \
                --net "host" \
                --env "ansible_user=${USER_NAME}" \
                --env "KUBESPRAY_NODES=${nodes}" \
                --volume "${KUBESPRAY_CONFIG}:/root/kiss/bootstrap/config.yaml:ro" \
                --volume "${KUBESPRAY_CONFIG_ALL}:/root/kiss/bootstrap/all.yaml:ro" \
                --volume "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/:/etc/kiss/bootstrap/:ro" \
                --volume "${SSH_KEYFILE}:/root/.ssh/id_ed25519:ro" \
                --volume "${SSH_KEYFILE}.pub:/root/.ssh/id_ed25519.pub:ro" \
                "${KUBESPRAY_IMAGE}" bash -c '
                    sed -i "s/\(^\- name\: reset | Restart network$\)/\1\n  ignore_errors\: true/g" /kubespray/roles/reset/tasks/main.yml \
                    && exec ansible-playbook \
                        --become --become-user="root" \
                        --inventory "/root/kiss/bootstrap/all.yaml" \
                        --inventory "/root/kiss/bootstrap/config.yaml" \
                        "/etc/kiss/bootstrap/roles/reset-k8s.yaml"
                '
        fi

        # Install cluster
        __log 'INFO' "Installing k8s cluster ..."
        ${CONTAINER_RUNTIME} run --rm --tty \
            --name "k8s-installer" \
            --net "host" \
            --env "ansible_user=${USER_NAME}" \
            --env "KUBESPRAY_NODES=${nodes}" \
            --volume "${KUBESPRAY_CONFIG}:/root/kiss/bootstrap/config.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_ALL}:/root/kiss/bootstrap/all.yaml:ro" \
            --volume "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/:/etc/kiss/bootstrap/:ro" \
            --volume "${SSH_KEYFILE}:/root/.ssh/id_ed25519:ro" \
            --volume "${SSH_KEYFILE}.pub:/root/.ssh/id_ed25519.pub:ro" \
            "${KUBESPRAY_IMAGE}" ansible-playbook \
            --become --become-user="root" \
            --inventory "/root/kiss/bootstrap/all.yaml" \
            --inventory "/root/kiss/bootstrap/config.yaml" \
            "/etc/kiss/bootstrap/roles/install-k8s.yaml"

        # Upload kubespray config into nodes
        __log 'PATCH' "Uploading kubespray configurations ..."
        if [ "x${INSTALLER_TYPE}" = 'xhost' ]; then
            $(__shell "${node_first}") \
                cp -r "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/" '/etc/kiss'
        fi
        $(__shell "${node_first}") \
            mkdir -p "/root/kiss/bootstrap/"
        $(__shell "${node_first}") \
            tee "/root/kiss/bootstrap/all.yaml" \
            <"${KUBESPRAY_CONFIG_ALL}"
        $(__shell "${node_first}") \
            tee "/root/kiss/bootstrap/config.yaml" \
            <"${KUBESPRAY_CONFIG}"

        # Download k8s config into host
        __log 'PATCH' "Downloading kubernetes config file ..."
        mkdir -p "${KUBERNETES_CONFIG}"
        $(__shell "${node_first}") tar -cf - -C "/root/.kube" "." |
            tar -xf - -C "${KUBERNETES_CONFIG}"

        # Cleanup
        __log 'PATCH' "Cleaning up configurations ..."
        rm -rf "${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/"
    fi

    # Finished!
    __log 'DONE' "Installed k8s cluster"
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
        $(__shell "${node_first}") \
            kubectl get namespaces kiss \
            >/dev/null 2>/dev/null
    then
        __log 'SKIP' "Using already installed kiss cluster ..."
        local NEED_INSTALL=0
    fi

    if [ "x${NEED_INSTALL}" == "x1" ]; then
        # Upload the K8S Configuration File to the Cluster
        __log 'PATCH' "Uploading kubernetes configurations to the cluster ..."
        $(__shell "${node_first}") \
            kubectl create namespace kiss
        cat "${KISS_CONFIG_PATH}" |
            ${CONTAINER_RUNTIME} run --interactive --rm "${YQ_IMAGE}" \
                "(select(.kind == \"ConfigMap\") | .data.auth_ssh_key_id_ed25519_public) = \"$(
                    cat "${SSH_KEYFILE}.pub" |
                        awk '{print $1 " " $2}'
                )\"" |
            ${CONTAINER_RUNTIME} run --interactive --rm "${YQ_IMAGE}" \
                "(select(.kind == \"ConfigMap\") | .data.kiss_cluster_name) = \"default\"" |
            ${CONTAINER_RUNTIME} run --interactive --rm "${YQ_IMAGE}" \
                "(select(.kind == \"Secret\") | .stringData.auth_ssh_key_id_ed25519) = \"$(
                    cat "${SSH_KEYFILE}"
                )\n\"" |
            $(__shell "${node_first}") \
                kubectl apply -f -
        $(__shell "${node_first}") \
            kubectl create -n kiss configmap "ansible-control-planes-default" \
            "--from-file=defaults.yaml=/etc/kiss/bootstrap/defaults/all.yaml" \
            "--from-file=hosts.yaml=/etc/kiss/bootstrap/inventory/hosts.yaml" \
            "--from-file=all.yaml=/root/kiss/bootstrap/all.yaml" \
            "--from-file=config.yaml=/root/kiss/bootstrap/config.yaml"

        # Install cluster
        __log 'INFO' "Installing kiss cluster ..."
        ${CONTAINER_RUNTIME} run --rm --tty \
            --name "kiss-installer" \
            --net "host" \
            --pull "always" \
            --volume "${KUBERNETES_CONFIG}:/root/.kube:ro" \
            "${KISS_INSTALLER_IMAGE}"

        # Wait for kiss operator to run
        __log 'INFO' "Waiting for kiss operator to run ..."
        $(__shell "${node_first}") \
            kubectl rollout status deployment operator

        # Register the boxes
        for node in ${nodes}; do
            __log 'INFO' "Registering the box: ${node} ..."
            cat <<EOF | $(__shell "${node_first}") kubectl create box "${node}"
---
apiVersion: kiss.ulagbulag.io/v1alpha1
kind: Box
metadata:
    name: "${node}"
    labels:
        kiss.ulagbulag.io/verify-bind-group: "false"
spec:
    group:
        clusterName: default
        role: ControlPlane
    machine:
        uuid: "${node}"
EOF
        done

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
    __log 'DONE' "Installed kiss cluster"
}

###########################################################
#   Build an Installer ISO                                #
###########################################################

function build_installer_iso() {
    # Define variables
    local BOOT_IPXE_FILE='boot-rocky9.ipxe'
    local BOOT_KICKSTART_FILE='rocky9.ks'
    local KUBESPRAY_CONFIG_TEMPLATE='/etc/openark/kiss/'

    # Parse variables
    local BOOT_NETWORK_IPV4_ADDRESS="$(kiss_config 'bootstrapper_network_ipv4_address')"
    local BOOT_NETWORK_IPV4_GATEWAY="$(kiss_config 'network_ipv4_gateway')"
    local BOOT_NETWORK_IPV4_NETMASK="$(
        docker run --rm "${IPCALC_IMAGE}" "$(kiss_config 'network_ipv4_subnet')" |
            grep -Po 'Netmask\: +\K[0-9\.]+'
    )"
    local BOOT_NETWORK_DNS_SERVER_NS1="$(kiss_config 'bootstrapper_network_dns_server_ns1')"
    local BOOT_NETWORK_DNS_SERVER_NS2="$(kiss_config 'bootstrapper_network_dns_server_ns2')"
    local BOOT_NETWORK_MTU="$(kiss_config 'network_interface_mtu_size')"
    local SSH_KEYFILE="$(realpath $(kiss_config 'bootstrapper_auth_ssh_key_path'))"

    # Update variables
    kiss_patch_config 'bootstrapper_auth_ssh_key_path' "\"${KUBESPRAY_CONFIG_TEMPLATE}/id_ed25519\""
    kiss_patch_config 'bootstrapper_kubernetes_config_path' "\"/root/.kube\""
    kiss_patch_config 'bootstrapper_kubespray_config_all_path' "\"${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/defaults/all.yaml\""
    kiss_patch_config 'bootstrapper_kubespray_config_path' "\"${KUBESPRAY_CONFIG_TEMPLATE}/bootstrap/defaults/all.yaml\""
    kiss_patch_config 'bootstrapper_kubespray_config_template_path' "\"${KUBESPRAY_CONFIG_TEMPLATE}\""

    ROOTFS="$(pwd)/config/rootfs"
    __log 'INFO' "Create and Enter into a rootfs directory ..."
    rm -rf "${ROOTFS}"
    mkdir -p "${ROOTFS}"
    pushd "${ROOTFS}" >/dev/null

    SCRIPTS_HOME="$(pwd)/../../../kiss/matchbox/boot/"
    __log 'INFO' "Copying install scripts from \"${SCRIPTS_HOME}\" ..."
    cp -arp ${SCRIPTS_HOME}/* "${ROOTFS}"

    __log 'PATCH' "Parsing boot scripts ..."
    local re_url='[0-9a-zA-Z:/\.\$\{\}]*'
    local boot_dist_repo="$(cat "${BOOT_IPXE_FILE}" | grep -Po "inst.repo=\K${re_url}")"

    __log 'PATCH' "Removing unneeded scripts ..."
    rm -rf ${ROOTFS}/*.ipxe

    __log 'INFO' "Applying SSH Keys into scripts ..."
    sed -i "s/ENV_USERNAME/$(kiss_config 'auth_ssh_username')/g" ./*
    sed -i "s/ENV_SSH_AUTHORIZED_KEYS/$(kiss_config 'auth_ssh_key_id_ed25519_public')/g" ./*

    __log 'INFO' "Enabling static network interface ..."
    sed -i "s/^\(network .*\)$/\#\1\nnetwork --activate --bootproto=static --ip=${BOOT_NETWORK_IPV4_ADDRESS} --netmask=${BOOT_NETWORK_IPV4_NETMASK} --gateway=${BOOT_NETWORK_IPV4_GATEWAY} --nameserver=${BOOT_NETWORK_DNS_SERVER_NS1},${BOOT_NETWORK_DNS_SERVER_NS2} --mtu=${BOOT_NETWORK_MTU}/g" "${ROOTFS}/${BOOT_KICKSTART_FILE}"

    __log 'INFO' "Enabling auto-deployment of KISS cluster ..."
    sed -i 's/^\(\%end \+\)#\( \+\)SCRIPT_END$/\#\1\#\2SCRIPT_CONTINUE/g' "${ROOTFS}/${BOOT_KICKSTART_FILE}"
    cat <<EOF >>"${ROOTFS}/${BOOT_KICKSTART_FILE}"

# Disable Box Discovery
rm /etc/systemd/system/multi-user.target.wants/notify-new-box.service

# KISS Cluster Installation Script
cat <<__EOF__ >/etc/systemd/system/init-new-cluster.service
[Unit]
Description=Create a new KISS cluster.
Wants=network-online.target
After=network-online.target

[Service]
Type=oneshot
Environment="CONTAINER_RUNTIME=${CONTAINER_RUNTIME}"
Environment="INSTALLER_TYPE=host"
Environment="KISS_CONFIG_PATH=${KUBESPRAY_CONFIG_TEMPLATE}/$(basename "${KISS_CONFIG_PATH}")"
ExecStart=/bin/bash -c " \
    ls /etc/systemd/system/multi-user.target.wants/kubelet.service >/dev/null || \
        curl --retry 5 --retry-delay 5 -sS "${KISS_BOOTSTRAPPER_URL}" | bash \
"
Restart=on-failure
RestartSec=30

[Install]
WantedBy=multi-user.target
__EOF__
ln -sf /usr/lib/systemd/system/init-new-cluster.service /etc/systemd/system/multi-user.target.wants/init-new-cluster.service

# KISS Cluster Configuration File
mkdir -p "${KUBESPRAY_CONFIG_TEMPLATE}"
chmod 700 "${KUBESPRAY_CONFIG_TEMPLATE}"
cat <<__EOF__ >${KUBESPRAY_CONFIG_TEMPLATE}/$(basename "${KISS_CONFIG_PATH}")
$(cat "${KISS_CONFIG_PATH}")
__EOF__

# KISS Keyfile
cat <<__EOF__ >${KUBESPRAY_CONFIG_TEMPLATE}/$(basename "${SSH_KEYFILE}")
$(cat "${SSH_KEYFILE}")
__EOF__

%end  # SCRIPT_END

EOF

    __log 'INFO' "Adding KISS Cluster Configuration File ..."
    cp "${KISS_CONFIG_PATH}" ./

    __log 'INFO' "Adding Keyfile ..."
    cp "${SSH_KEYFILE}" ./

    __log 'INFO' "Adding grub.cfg ..."
    cat <<EOF >"${ROOTFS}/grub.cfg"
set default="1"

function load_video {
  insmod efi_gop
  insmod efi_uga
  insmod video_bochs
  insmod video_cirrus
  insmod all_video
}

load_video
set gfxpayload=keep
insmod gzio
insmod part_gpt
insmod ext2

set timeout=3

linuxefi /images/pxeboot/vmlinuz inst.ks=cdrom:/EFI/BOOT/${BOOT_KICKSTART_FILE}
initrdefi /images/pxeboot/initrd.img
boot

EOF

    __log 'INFO' "Adding isolinux.cfg ..."
    cat <<EOF >"${ROOTFS}/isolinux.cfg"
default vesamenu.c32
timeout 3

display boot.msg

kernel vmlinuz
append initrd=initrd.img inst.ks=cdrom:/EFI/BOOT/${BOOT_KICKSTART_FILE}
boot

EOF

    __log 'INFO' "Finished Patching!"
    popd

    ISO_BASE_PATH="$(pwd)/config/base.iso"
    __log 'PATCH' "Downloading base ISO ..."
    if [ ! -f "${ISO_BASE_PATH}" ]; then
        curl -o "${ISO_BASE_PATH}" "${ISO_BASE_URL}"
    fi

    __log 'INFO' "Patching ISO ..."
    INSTALLER_PATH="$(pwd)/config/OpenARK-$(date -u +%y.%m-%d)-server-$(uname -m).iso"
    rm -f "${INSTALLER_PATH}"
    ln -sf "${INSTALLER_PATH}" "${INSTALLER_PATH}/../installer.iso" 2>/dev/null || true
    ${CONTAINER_RUNTIME} run --rm \
        --volume "${ISO_BASE_PATH}/..:/img/src" \
        --volume "${INSTALLER_PATH}/..:/img/dst" \
        --volume "${ROOTFS}:/src" \
        "${XORRISO_IMAGE}" xorriso \
        -boot_image isolinux patch \
        -indev "/img/src/$(basename "${ISO_BASE_PATH}")" \
        -outdev "/img/dst/$(basename "${INSTALLER_PATH}")" \
        -map "/src/" "/EFI/BOOT/" \
        -map "/src/isolinux.cfg" "/isolinux/isolinux.cfg" \
        -rm "/EFI/BOOT/isolinux.cfg"
}

###########################################################
#   Main Function                                         #
###########################################################

# Define a main function
function main() {
    # Check host dependencies
    check_dependencies

    case "x${INSTALLER_TYPE}" in
    "xcontainer")
        # Validate Configurations
        kiss_validate_config_file

        # Configure Host
        configure_env
        configure_linux_kernel
        generate_ssh_keypair

        # Spawn k8s cluster nodes
        export nodes # results
        if [ "x${KUBESPRAY_NODES}" = 'xhost ' ]; then
            export INSTALLER_TYPE='host'
            export KUBESPRAY_NODES="$(sudo cat /sys/class/dmi/id/product_uuid)"
            spawn_node_on_host ${KUBESPRAY_NODES}
        else
            for name in ${KUBESPRAY_NODES}; do
                spawn_node_on_container "${name}"
            done
        fi

        # Install a k8s cluster within nodes
        install_k8s_cluster ${KUBESPRAY_NODES}

        # Install a KISS cluster within k8s cluster
        install_kiss_cluster ${KUBESPRAY_NODES}

        # Finished!
        __log 'DONE' "Completed!"
        ;;
    "xhost")
        # Set default node name to 'host'
        export KUBESPRAY_NODES="host"

        # Same as `container`
        export INSTALLER_TYPE='container'
        main "$@"
        ;;
    "xiso")
        # Validate Configurations
        kiss_validate_config_file

        # Configure Host
        generate_ssh_keypair

        # Build an Installer ISO
        build_installer_iso

        # Finished!
        __log 'DONE' "Completed!"
        ;;
    *)
        __log 'ERROR' "Unsupported installer type: ${INSTALLER_TYPE}; Aborting."
        ;;
    esac
}

# Execute main function
main "$@"
