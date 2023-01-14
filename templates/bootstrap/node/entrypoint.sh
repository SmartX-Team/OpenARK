#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e

# Skip re-initialization
if [ ! -f "${HOME}/.ssh/id_ed25519" ]; then
    # Port scanner
    function get_available_port() {
        comm -23 <(seq 49152 65535 | sort) <(ss -Htan | awk '{print $4}' | cut -d':' -f2 | sort -u) | shuf | head -n 1
    }

    # Generate SSH keys
    ssh-keygen -q -t ed25519 -f "${HOME}/.ssh/id_ed25519" -N ''
    ssh-keygen -q -A

    # Register the given public SSH key as authorized
    if [ ! "${SSH_PUBKEY}" ]; then
        echo "Error: SSH Public Key (\$SSH_PUBKEY) is not given!"
        exit 1
    fi
    echo "${SSH_PUBKEY}" >>"${HOME}/.ssh/authorized_keys"
    chmod 600 "${HOME}/.ssh/authorized_keys"

    # Find an available SSH port
    while [ ! "$ssh_port" ]; do
        ssh_port=$(get_available_port)
    done

    # Apply the SSH port
    sed -i "s/^#\(Port\) 22/\1 ${ssh_port}/g" "/etc/ssh/sshd_config"
fi

# Replace /etc/hostname to local
cp /etc/hostname /tmp/hostname &&
    umount /etc/hostname &&
    mv --force /tmp/hostname /etc/hostname

# Replace /etc/hosts to local
cp /etc/hosts /tmp/hosts &&
    umount /etc/hosts &&
    mv --force /tmp/hosts /etc/hosts

# Replace /etc/resolv.conf to local
cp /etc/resolv.conf /tmp/resolv.conf &&
    umount /etc/resolv.conf &&
    mv --force /tmp/resolv.conf /etc/resolv.conf

# Execute systemd
exec /usr/sbin/init
