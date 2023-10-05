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

###########################################################
#   SSH Configuration                                     #
###########################################################

# Create SSH daemon home
if [ ! -d "/run/sshd" ]; then
    mkdir /run/sshd
fi

# Change user home directory permissions
chown 'user:users' "${USER_HOME}"

# Generate Host SSH keys
if [ ! -f "/etc/ssh/ssh_host_ed25519_key.pub" ]; then
    cp -r /etc/.ssh/* /etc/ssh
    ssh-keygen -q -A
fi
rm -rf /etc/.ssh

# Generate User SSH keys
if [ ! -f "${USER_HOME}/.ssh/id_ed25519" ]; then
    su user -c "ssh-keygen -q -t ed25519 -f '${USER_HOME}/.ssh/id_ed25519' -N ''"
fi

###########################################################
#   Copy podman containers configuration file             #
###########################################################

if which podman; then
    if [ ! -d "${USER_HOME}/.config/containers" ]; then
        su user -c "mkdir -p '${USER_HOME}/.config/containers'"
        su user -c "rm -rf '${USER_HOME}/.config/containers/containers.conf'"
        su user -c "cp /etc/containers/podman-containers.conf '${USER_HOME}/.config/containers/containers.conf'"
    fi

    # Initialize rootless podman
    su user -c "podman system migrate"

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
#   Run SSH Server                                        #
###########################################################

exec /usr/sbin/sshd -D
