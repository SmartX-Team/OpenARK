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
CSV_PATH_DEFAULT="./list.csv"
SSH_KEYFILE_PATH_DEFAULT="${HOME}/.ssh/kiss"

# Configure environment variables
CSV_PATH="${CSV_PATH:-$CSV_PATH_DEFAULT}"
SSH_KEYFILE_PATH="${SSH_KEYFILE_PATH:-$SSH_KEYFILE_PATH_DEFAULT}"

###########################################################
#   Label all noxes with given Aliases                    #
###########################################################

echo 'Labeling boxes'
for line in $(cat "${CSV_PATH}" | tail '+2'); do
    box_name="$(echo "${line}" | cut '-d,' -f1)"
    box_alias="$(echo "${line}" | cut '-d,' -f2)"

    echo -n "* ${box_alias} -> "
    kubectl patch boxes "${box_name}" \
        --type 'merge' \
        --patch "{\"metadata\":{\"labels\":{\"dash.ulagbulag.io/alias\":\"${box_alias}\"}}}"
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
