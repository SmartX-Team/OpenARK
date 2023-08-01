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
COMMAND_DEFAULT="${@:1}"
CSV_PATH_DEFAULT="./users.csv"

# Configure environment variables
COMMAND="${COMMAND:-$COMMAND_DEFAULT}"
CSV_PATH="${CSV_PATH:-$CSV_PATH_DEFAULT}"

###########################################################
#   Run the given script for all given nodes              #
###########################################################

echo 'Running for user sessions'
for line in $(cat "${CSV_PATH}" | tail '+2'); do
  user_id="$(echo "${line}" | cut '-d,' -f1)"
  user_name="$(echo "${line}" | cut '-d,' -f2)"
  user_namespace="vine-session-${user_id}"

  echo -n "* ${user_name} -> "
  if [ "x$(
    kubectl get namespace "${user_namespace}" -o jsonpath --template '{.metadata.labels.ark\.ulagbulag\.io\/bind}'
  )" != 'xtrue' ]; then
    echo 'Skipping (Not Binded)'
    continue
  fi

  user_box="$(kubectl get namespace "${user_namespace}" -o jsonpath --template '{.metadata.labels.ark\.ulagbulag\.io\/bind\.node}')"
  if [ "x${user_box}" = 'x' ]; then
    echo 'Skipping (No box is logged in)'
    continue
  fi
  echo -n "${user_box} -> "

  echo -n "job/desktop-${user_box} -> "
  kubectl exec \
    --namespace "${user_namespace}" \
    --container 'desktop-environment' \
    --quiet \
    "job/desktop-${user_box}" -- ${COMMAND} &
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
sleep infinity
