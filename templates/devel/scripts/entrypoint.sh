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

# Remove old ssh server configuration
rm -rf /etc/ssh/sshd_config

# Grant ssh access for user
mkdir /run/sshd
chown 'user:users' "/home/user"

# Generate SSH keys
ssh-keygen -q -A
su user ssh-keygen -q -t ed25519 -f "/home/user/.ssh/id_ed25519" -N ''

###########################################################
#   Run SSH Server                                        #
###########################################################

exec /usr/sbin/sshd -D
