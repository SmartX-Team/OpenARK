#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
HELM_CHART_DEFAULT="https://operator.min.io"
NAMESPACE_DEFAULT="minio-operator"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

# Parse from CoreDNS
export CLUSTER_NAME="$(
    kubectl -n kube-system get configmap coredns -o yaml |
        yq -r '.data.Corefile // ""' |
        grep -Po ' +kubernetes \K[\w\.\_\-]+'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${CLUSTER_NAME}" == "x" ]; then
    echo 'Skipping installation: "CLUSTER_NAME" not set'
    exit 0
fi

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}" "${HELM_CHART}"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "operator" \
    "${NAMESPACE}/operator" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set operator.env[0].value="${CLUSTER_NAME}" \
    --values "./values-operator.yaml"

# Finished!
echo "Installed!"
