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
HELM_CHART_DEFAULT="https://helm.ngc.nvidia.com/nvidia"
NAMESPACE_DEFAULT="gpu-nvidia"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}" "${HELM_CHART}"

###########################################################
#   Configure Helm Values                                 #
###########################################################

echo "- Configuring Helm values ... "

TOOLKIT_VERSION="$(
    helm show values gpu-nvidia/gpu-operator |
        yq '.toolkit.version' |
        grep -Po '^v[0-9\.]+'
)"

###########################################################
#   Checking if Operator is already installed             #
###########################################################

echo "- Checking Operator is already installed ... "
if
    kubectl get namespace --no-headers "${NAMESPACE}" \
        >/dev/null 2>/dev/null
then
    IS_FIRST=0
else
    IS_FIRST=1
fi

###########################################################
#   Install NodeFeatures                                  #
###########################################################

kubectl apply \
    --kustomize "https://github.com/kubernetes-sigs/node-feature-discovery/deployment/overlays/default"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "gpu-operator" \
    "${NAMESPACE}/gpu-operator" \
    --create-namespace \
    --disable-openapi-validation \
    --namespace "${NAMESPACE}" \
    --set toolkit.version="${TOOLKIT_VERSION}-ubi8" \
    --values "./values-operator.yaml"

# Finished!
echo "Installed!"
