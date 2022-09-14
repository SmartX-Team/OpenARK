#!/bin/bash

# Prehibit errors
set -e

# Configure default environment variables
CONTAINER_RUNTIME_DEFAULT="docker"
KUBESPRAY_CONFIG_DEFAULT="$(pwd)/config/bootstrap/defaults/hosts.yaml"
KUBESPRAY_CONFIG_TEMPLATE_DEFAULT="$(pwd)/config/"
KUBESPRAY_IMAGE_DEFAULT="quay.io/kubespray/kubespray:v2.19.1"
KUBESPRAY_NODES_DEFAULT="master1"
REUSE_NODES_DEFAULT="true"
SSH_KEYFILE_DEFAULT="$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT/id_rsa"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
KUBESPRAY_CONFIG="${KUBESPRAY_CONFIG:-$KUBESPRAY_CONFIG_DEFAULT}"
KUBESPRAY_CONFIG_TEMPLATE="${KUBESPRAY_CONFIG_TEMPLATE:-$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT}"
KUBESPRAY_IMAGE="${KUBESPRAY_IMAGE:-$KUBESPRAY_IMAGE_DEFAULT}"
KUBESPRAY_NODES="${KUBESPRAY_NODES:-$KUBESPRAY_NODES_DEFAULT}"
REUSE_NODES="${REUSE_NODES:-$REUSE_NODES_DEFAULT}"
SSH_KEYFILE="${SSH_KEYFILE:-$SSH_KEYFILE_DEFAULT}"

# Check linux dependencies
UNAME="$(uname -r)"
if [ ! -d "/usr/src/linux-headers-$UNAME" ]; then
    echo "Error: Cannot find the linux modules (/lib/modules/$UNAME)"
    echo "Note: You may reboot your machine to reload the kernel."
    exit 1
fi
if [ ! -d "/usr/src/linux-headers-$UNAME" ]; then
    echo "Error: Cannot find the linux header (/usr/src/linux-headers-$UNAME)"
    echo "Note: You may install it via your preferred package manager."
    exit 1
fi

# Generate a SSH keypair
if [ ! -f "${SSH_KEYFILE_DEFAULT}" ]; then
    echo "Generating a SSH Keypair..."
    mkdir -p "$KUBESPRAY_CONFIG_TEMPLATE_DEFAULT"
    ssh-keygen -q -t rsa -f "$SSH_KEYFILE" -N ''
fi

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

    # Spawn a node
    if [ "$NEED_SPAWN" -eq 1 ]; then
        echo -n "- Spawning a node ($name) ... "
        "$CONTAINER_RUNTIME" run --detach \
            --name "$name" \
            --cgroupns "host" \
            --hostname "$name.control.box.netai-cloud" \
            --ipc "host" \
            --net "host" \
            --privileged \
            --env "SSH_PUBKEY=$(cat ${SSH_KEYFILE}.pub)" \
            --tmpfs "/run" \
            --volume "/lib/modules/$UNAME:/lib/modules/$UNAME:ro" \
            --volume "/sys/fs/cgroup:/sys/fs/cgroup" \
            --volume "/usr/src/linux-headers-$UNAME:/usr/src/linux-headers-$UNAME" \
            node >/dev/null
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

# Define a k8s cluster installer function
function install_k8s_cluster() {
    local names="$1"
    local node_first="$(echo $names | awk '{print $1}')"

    # Cleanup
    rm -rf "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"

    # Get a sample kubespray configuration file
    mkdir -p "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"
    "$CONTAINER_RUNTIME" exec "$node_first" \
        tar -cf - -C "/etc/kiss/bootstrap/" "." |
        tar -xf - -C "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"

    # Spawn a node
    echo "- Spawning a k8s cluster ... "
    "$CONTAINER_RUNTIME" run --rm \
        --name "k8s-installer" \
        --net "host" \
        --env "KUBESPRAY_NODES=$nodes" \
        --volume "$KUBESPRAY_CONFIG:/root/kiss/bootstrap/hosts.yaml:ro" \
        --volume "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/:/etc/kiss/bootstrap/:ro" \
        --volume "$SSH_KEYFILE:/root/.ssh/id_rsa:ro" \
        --volume "$SSH_KEYFILE.pub:/root/.ssh/id_rsa.pub:ro" \
        $KUBESPRAY_IMAGE ansible-playbook \
        --become --become-user="root" \
        --inventory "/etc/kiss/bootstrap/defaults/hosts.yaml" \
        --inventory "/root/kiss/bootstrap/hosts.yaml" \
        "/etc/kiss/bootstrap/roles/install-k8s.yaml"

    # Cleanup
    rm -rf "$KUBESPRAY_CONFIG_TEMPLATE/bootstrap/"

    # Finished!
    echo "OK"
}

# Install a k8s cluster within nodes
install_k8s_cluster "$KUBESPRAY_NODES"
