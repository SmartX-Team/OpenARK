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
DST_PATH_DEFAULT="./result-reduce-$(date -u +%s).csv"
SCRIPT_DST_DEFAULT="/tmp/reduce-$(date -u +'%Y%m%dT%H%M%SZ').sh"
SCRIPT_PATH_DEFAULT="./batch-reduce-template.sh"
SSH_KEYFILE_PATH_DEFAULT="${HOME}/.ssh/kiss"

# Configure environment variables
CSV_PATH="${CSV_PATH:-$CSV_PATH_DEFAULT}"
DST_PATH="${DST_PATH:-$DST_PATH_DEFAULT}"
SCRIPT_DST="${SCRIPT_DST:-$SCRIPT_DST_DEFAULT}"
SCRIPT_PATH="${SCRIPT_PATH:-$SCRIPT_PATH_DEFAULT}"
SSH_KEYFILE_PATH="${SSH_KEYFILE_PATH:-$SSH_KEYFILE_PATH_DEFAULT}"

###########################################################
#   Run the given script for all given boxes              #
###########################################################

echo 'box,label,value' >>"${DST_PATH}"

echo 'Running for boxes'
for line in $(cat "${CSV_PATH}" | tail '+2'); do
  box_id="$(echo "${line}" | cut '-d,' -f1)"
  box_name="$(echo "${line}" | cut '-d,' -f2)"

  echo -n "* ${box_name} -> "
  echo -n "${box_id},${box_name}," >>"${DST_PATH}"

  address="$(kubectl get box "${box_id}" -o jsonpath='{.status.access.primary.address}')"
  if [ "x${address}" = "x" ]; then
    echo 'Not registered'
    echo >>"${DST_PATH}"
    continue
  fi

  ssh-keygen -f "${HOME}/.ssh/known_hosts" -R "${address}" >/dev/null 2>/dev/null

  if
    ping -c 1 -W 3 "${address}" >/dev/null 2>/dev/null &&
      ssh -i "${SSH_KEYFILE_PATH}" -o StrictHostKeyChecking=no "kiss@${address}" echo "Connected" 2>/dev/null \
      ;
  then
    scp -i "${SSH_KEYFILE_PATH}" -o StrictHostKeyChecking=no "${SCRIPT_PATH}" "kiss@${address}:${SCRIPT_DST}" >/dev/null
    if ssh -i "${SSH_KEYFILE_PATH}" -o StrictHostKeyChecking=no "kiss@${address}" bash "${SCRIPT_DST}" >>"${DST_PATH}"; then
      echo "OK"
    else
      echo >>"${DST_PATH}"
    fi
  else
    echo "Skipped"
    echo >>"${DST_PATH}"
  fi
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
