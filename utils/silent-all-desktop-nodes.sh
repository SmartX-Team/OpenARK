#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
ALERTMANAGER_URL_DEFAULT="http://alertmanager-operated.monitoring.svc:9093"
CREATOR_NAME_DEFAULT="OpenARK"

# Configure environment variables
ALERTMANAGER_URL="${ALERTMANAGER_URL:-$ALERTMANAGER_URL_DEFAULT}"
CREATOR_NAME="${CREATOR_NAME:-$CREATOR_NAME_DEFAULT}"

###########################################################
#   Silent all Desktop nodes                              #
###########################################################

for name in $(kubectl get box --no-headers -o name); do
    # Select all desktop nodes
    if [ "$(kubectl get "${name}" -o jsonpath --template '{.spec.group.role}')" != 'Desktop' ]; then
        continue
    fi

    # Collect the infomation
    box_name="${name##*/}"
    box_alias="$(kubectl get "${name}" -o jsonpath --template '{.metadata.labels.dash\.ulagbulag\.io/alias}')"
    if [ "x${box_alias}" == 'x' ]; then
        box_alias="${box_name}"
    fi

    # Make a query
    query='{
        "matchers": [
            {
                "name": "node",
                "value": "'"${box_name}"'",
                "isRegex": false,
                "isEqual": true
            }
        ],
        "startsAt": "'"$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")"'",
        "endsAt": "2099-12-31T23:59:59.999Z",
        "createdBy": "'"${CREATOR_NAME}"'",
        "comment": "Mute All Desktop Nodes ('"${box_alias}"')",
        "id": null
    }'

    # Commit
    curl -sL "${ALERTMANAGER_URL}/api/v2/silences" -X 'POST' \
        --json "${query}"
done
