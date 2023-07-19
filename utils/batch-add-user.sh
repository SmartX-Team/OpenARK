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
CSV_PATH_DEFAULT="./users.csv"

# Configure environment variables
CSV_PATH="${CSV_PATH:-$CSV_PATH_DEFAULT}"
TEMPLATE="
---
apiVersion: vine.ulagbulag.io/v1alpha1
kind: User
metadata:
  name: \"__ID__\"
spec:
  contact: {}
  detail: {}
  name: \"__NAME__\"
---
apiVersion: vine.ulagbulag.io/v1alpha1
kind: UserBoxQuotaBinding
metadata:
  name: \"__ID__-desktop\"
spec:
  quota: \"__QUOTA__\"
  user: \"__ID__\"
"

###########################################################
#   Label all noxes with given Aliases                    #
###########################################################

echo 'Creating users'
for line in $(cat "${CSV_PATH}" | tail '+2'); do
  user_id="$(echo "${line}" | cut '-d,' -f1)"
  user_name="$(echo "${line}" | cut '-d,' -f2)"
  user_quota="$(echo "${line}" | cut '-d,' -f3)"

  echo -n "* ${user_name} (${user_quota}) -> "
  echo "${TEMPLATE}" |
    sed "s/__ID__/${user_id}/g" |
    sed "s/__NAME__/${user_name}/g" |
    sed "s/__QUOTA__/${user_quota}/g" |
    kubectl apply -f -
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
