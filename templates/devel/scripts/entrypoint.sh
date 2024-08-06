#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Configure environment variables
USER_HOME="/home/user"
USER_NAME="${USER_HOME##*/}"
USER_ID="$(id -u "${USER_NAME}")"

###########################################################
#   SSH Configuration                                     #
###########################################################

# Create SSH daemon home
if [ ! -d "/run/sshd" ]; then
    mkdir /run/sshd
fi

# Change user home directory permissions
chown "${USER_NAME}:${USER_NAME}" "${USER_HOME}"

# Generate Host SSH keys
if [ ! -f "/etc/ssh/ssh_host_ed25519_key.pub" ]; then
    cp -r /etc/.ssh/* /etc/ssh
    ssh-keygen -q -A
fi
rm -rf /etc/.ssh

# Generate User SSH keys
if [ ! -f "${USER_HOME}/.ssh/id_ed25519" ]; then
    su "${USER_NAME}" -c "ssh-keygen -q -t ed25519 -f '${USER_HOME}/.ssh/id_ed25519' -N ''"
fi

###########################################################
#   Create User Password                                  #
###########################################################

if [ "x${USER_PASSWORD}" != 'x' ]; then
    echo -e "${USER_PASSWORD}\n${USER_PASSWORD}\n" |
        sudo passwd "${USER_NAME}"
fi

###########################################################
#   Change User Shell                                     #
###########################################################

if [ "x${USER_SHELL}" != 'x' ]; then
    chsh --shell "${USER_SHELL}" "${USER_NAME}"
fi

###########################################################
#   Copy podman containers configuration file             #
###########################################################

if which podman; then
    if [ ! -d "${USER_HOME}/.config/containers" ]; then
        su "${USER_NAME}" -c "mkdir -p '${USER_HOME}/.config/containers'"
        su "${USER_NAME}" -c "rm -rf '${USER_HOME}/.config/containers/containers.conf'"
        su "${USER_NAME}" -c "cp /etc/containers/podman-containers.conf '${USER_HOME}/.config/containers/containers.conf'"
    fi

    # Rootless Docker (Podman) Configuration
    sed -i '/^keyring/ d' /etc/containers/containers.conf
    sed -i 's/^\[containers\]/\0\nkeyring=false/g' /etc/containers/containers.conf
    sed -i '/^no_pivot_root/ d' /etc/containers/containers.conf
    sed -i 's/^\[engine\]/\0\nno_pivot_root=true/g' /etc/containers/containers.conf

    # Initialize rootless podman
    su "${USER_NAME}" -c "podman system migrate"

    # Generate a CDI specification that refers to all NVIDIA devices
    if [ ! -f "/etc/cdi/nvidia.json" ]; then
        chown -R root:root /etc/cdi
        if ! nvidia-ctk cdi generate --device-name-strategy=type-index --format=json >/etc/cdi/nvidia.json; then
            rm -f /etc/cdi/nvidia.json
        fi
    fi
fi

###########################################################
#   Copy Hosts file                                       #
###########################################################

if [ -f "${USER_HOME}/.hosts" ]; then
    cat "${USER_HOME}/.hosts" >>/etc/hosts
fi

###########################################################
#   Run Docker Daemon                                     #
###########################################################

export XDG_RUNTIME_DIR="/run/user/${USER_ID}"
mkdir -p "${XDG_RUNTIME_DIR}"
chown "${USER_NAME}:${USER_NAME}" "${XDG_RUNTIME_DIR}"

export DOCKER_BIN="${USER_HOME}/.local/bin"
export DOCKER_HOST="unix://${XDG_RUNTIME_DIR}/docker.sock"
export PATH="${DOCKER_BIN}:${PATH}"
export SKIP_IPTABLES="1"

# Install rootless docker
if [ "x${DOCKER_ROOTLESS}" = 'xtrue' ]; then
    if docker --version | grep -q '^Docker'; then
        if [ ! -d "${DOCKER_BIN}" ]; then
            curl -fsSL 'https://get.docker.com/rootless' | su 'user' -s '/bin/bash'
        fi

        # Run rootless docker daemon
        su "${USER_NAME}" -c "env PATH=${PATH} dockerd-rootless.sh" &
    fi
fi

###########################################################
#   Share public environment variables                    #
###########################################################

for env_key in $(
    export |
        grep -Po '^declare \-x \K[a-zA-Z0-9_]+'
); do
    if [ "x${env_key}" = 'x' ]; then
        continue
    elif [ "x${env_key}" = 'xHOME' ]; then
        continue
    elif [ "x${env_key}" = 'xOLDPWD' ]; then
        continue
    elif [ "x${env_key}" = 'xPWD' ]; then
        continue
    elif [ "x${env_key}" = 'xSHLVL' ]; then
        continue
    elif echo "x${env_key}" | grep -q '^xUSER_'; then
        continue
    fi

    echo "${env_key}=\"${!env_key}\"" >>/etc/environment
done

###########################################################
#   Update ldconfig                                       #
###########################################################

ldconfig

###########################################################
#   Run SSH Server                                        #
###########################################################

exec /usr/sbin/sshd -D
