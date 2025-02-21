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

# Parse from kiss-config
export OS_DIST="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.os_dist // ""'
)"
export OS_VERSION="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.os_version // ""'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${OS_DIST}" == "x" ]; then
    echo 'Skipping installation: "OS_DIST" not set'
    exit 0
fi
if [ "x${OS_VERSION}" == "x" ]; then
    echo 'Skipping installation: "OS_VERSION" not set'
    exit 0
fi

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
    helm show values "${NAMESPACE}/gpu-operator" |
        yq '.toolkit.version' |
        grep -Po '^v[0-9\.]+'
)"
case "${OS_DIST}" in
"rocky")
    TOOLKIT_OS="ubi8"
    ;;
"ubuntu")
    TOOLKIT_OS="ubuntu20.04"
    ;;
*)
    echo "Unknown OS: ${OS_DIST}" >&2
    exit 1
    ;;
esac

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
    --set toolkit.version="${TOOLKIT_VERSION}-${TOOLKIT_OS}" \
    --values "./values-operator.yaml"

# Finished!
echo "Installed!"
