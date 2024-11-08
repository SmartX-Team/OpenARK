#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Test the sudo permission
if ! sudo whoami >/dev/null; then
    exec true
fi

# Create SSH daemon home
if [ ! -d "/run/sshd" ]; then
    sudo mkdir /run/sshd
fi

# Generate host SSH keys
if [ ! -f "/etc/ssh/ssh_host_ed25519_key.pub" ]; then
    cp -r /etc/.ssh/* /etc/ssh
    sudo ssh-keygen -q -A
fi

# Generate user SSH keys
if [ ! -f "${HOME}/.ssh/id_ed25519" ]; then
    ssh-keygen -q -t ed25519 -f "${HOME}/.ssh/id_ed25519" -N ''
fi

# Change user password
set +x
if [ "x${USER_PASSWORD}" != 'x' ]; then
    echo -e "${USER_PASSWORD}\n${USER_PASSWORD}\n" |
        sudo -S passwd "$(whoami)"
fi
unset USER_PASSWORD
set -x

# Change user shell
set +x
if [ "x${USER_SHELL}" != 'x' ]; then
    sudo chsh --shell "$(which "${USER_SHELL}")" "$(whoami)"
fi
unset USER_SHELL
set -x

# Copy hosts file
if [ -f "${HOME}/.hosts" ]; then
    cat "${HOME}/.hosts" | sudo tee -a /etc/hosts >/dev/null
fi

# Propagate environment variables to the session
cat "${__ENV_HOME}" |
    grep -Po '^export +\K.*$' |
    sudo tee -a /etc/environment >/dev/null

# Share public environment variables
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

    echo "${env_key}=\"${!env_key}\"" | sudo tee -a /etc/environment >/dev/null
done

# Run SSH Server
sudo /usr/sbin/sshd -D &
