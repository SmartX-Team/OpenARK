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
HELM_CHART_DEFAULT="https://nats-io.github.io/k8s/helm/charts"
NAMESPACE_DEFAULT="dash"
NATS_ENABLE_PVC_DEFAULT="true"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"
NATS_ENABLE_PVC="${NATS_ENABLE_PVC:-$NATS_ENABLE_PVC_DEFAULT}"

# Parse from CoreDNS
export CLUSTER_NAME="$(
    kubectl -n kube-system get configmap coredns -o yaml |
        yq -r '.data.Corefile' |
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

helm repo add "${NAMESPACE}-nats" "${HELM_CHART}"

###########################################################
#   Checking if Chart is already installed                #
###########################################################

echo "- Checking Chart is already installed ... "
if
    kubectl get namespace --no-headers "${NAMESPACE}" \
        >/dev/null 2>/dev/null
then
    IS_FIRST=0
else
    IS_FIRST=1
fi

###########################################################
#   Install Chart                                         #
###########################################################

echo "- Installing Chart ... "

helm upgrade --install "nats" \
    "${NAMESPACE}-nats/nats" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set config.cluster.routeURLs.k8sClusterDomain="${CLUSTER_NAME}" \
    --set config.jetstream.fileStore.pvc.enabled="${NATS_ENABLE_PVC}" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
