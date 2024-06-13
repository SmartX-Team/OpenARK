#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
SCRIPT_DST_DEFAULT="/tmp/patch-$(date -u +'%Y%m%dT%H%M%SZ').sh"
SCRIPT_PATH_DEFAULT="./patch-template.sh"
SSH_KEYFILE_PATH_DEFAULT="${HOME}/.ssh/kiss"

# Configure environment variables
SCRIPT_DST="${SCRIPT_DST:-$SCRIPT_DST_DEFAULT}"
SCRIPT_PATH="${SCRIPT_PATH:-$SCRIPT_PATH_DEFAULT}"
SSH_KEYFILE_PATH="${SSH_KEYFILE_PATH:-$SSH_KEYFILE_PATH_DEFAULT}"

###########################################################
#   Patch all nodes with Primary address via SSH          #
###########################################################

for address in $(kubectl get box -o jsonpath='{.items[*].status.access.primary.address}'); do
    name=$(kubectl get box | grep "${address}" | awk '{print $2}')

    echo -n "Patching \"${name}\"... "

    ssh-keygen -f "${HOME}/.ssh/known_hosts" -R "${address}" >/dev/null 2>/dev/null

    if
        ping -c 1 -W 3 "${address}" >/dev/null 2>/dev/null &&
            ssh -i "${SSH_KEYFILE_PATH}" -o StrictHostKeyChecking=no "kiss@${address}" echo "Connected" 2>/dev/null \
            ;
    then
        scp -i "${SSH_KEYFILE_PATH}" -o StrictHostKeyChecking=no "${SCRIPT_PATH}" "kiss@${address}:${SCRIPT_DST}"
        ssh -i "${SSH_KEYFILE_PATH}" -o StrictHostKeyChecking=no "kiss@${address}" sudo bash "${SCRIPT_DST}" || true
        echo "OK"
    else
        echo "Skipped"
    fi
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
