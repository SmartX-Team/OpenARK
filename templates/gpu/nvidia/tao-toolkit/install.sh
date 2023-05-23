#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
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
HELM_CHART_DEFAULT="https://helm.ngc.nvidia.com/nvidia/tao"
HELM_CHART_HOME_DEFAULT="./tao-toolkit-api"
HELM_CHART_VERSION_DEFAULT="4.0.0"
NAMESPACE_DEFAULT="tao-toolkit"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
HELM_CHART_HOME="${HELM_CHART_HOME:-$HELM_CHART_HOME_DEFAULT}"
HELM_CHART_VERSION="${HELM_CHART_VERSION:-$HELM_CHART_VERSION_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm fetch "${HELM_CHART}/charts/tao-toolkit-api-${HELM_CHART_VERSION}.tgz"
mkdir -p "${HELM_CHART_HOME}"
tar -zxf "./tao-toolkit-api-${HELM_CHART_VERSION}.tgz" -C "."

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
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "tao-toolkit-api" \
    "${HELM_CHART_HOME}" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --values "./values-operator.yaml"

# Finished!
echo "Installed!"
