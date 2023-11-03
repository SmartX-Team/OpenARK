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
NAMESPACE_DEFAULT="dev"

# Set environment variables
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Install Namespace                                     #
###########################################################

echo "- Installing namespace ... "

if
    ! kubectl get namespace "${NAMESPACE}" \
        >/dev/null 2>/dev/null
then
    kubectl create namespace "${NAMESPACE}"
fi

###########################################################
#   Install Templates                                     #
###########################################################

echo "- Installing templates ... "

for file in ./templates/*.yaml.j2; do
    filename="$(basename "${file}")"
    name="${filename/.yaml.j2/}"

    echo -e -n '  * '
    kubectl create configmap "${name}" \
        --namespace="${NAMESPACE}" \
        --from-file="${file}" \
        --output=yaml \
        --dry-run=client |
        kubectl apply -f -
done

###########################################################
#   Install Tasks                                         #
###########################################################

echo "- Installing tasks ... "

for file in ./tasks/*.yaml; do
    echo -e -n '  * '
    kubectl apply -f "${file}"
done

# Finished!
echo "Installed!"
