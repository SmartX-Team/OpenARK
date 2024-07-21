#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
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
HELM_CHART_DEFAULT="https://helm-repository.readthedocs.io/en/latest/repos/stable/"
NAMESPACE_DEFAULT="kubevirt"
TAG_CDI_DEFAULT="$(curl -s -w '%{redirect_url}' 'https://github.com/kubevirt/containerized-data-importer/releases/latest')"
VERSION_DEFAULT="$(curl -s 'https://storage.googleapis.com/kubevirt-prow/release/kubevirt/kubevirt/stable.txt')"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"
VERSION="${VERSION:-$VERSION_DEFAULT}"
VERSION_CDI="$(echo "${TAG_CDI_DEFAULT##*/}")"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}" "${HELM_CHART}"

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

kubectl apply \
    -f "https://github.com/kubevirt/kubevirt/releases/download/${VERSION}/kubevirt-operator.yaml"

###########################################################
#   Install Operator CR                                   #
###########################################################

echo "- Installing Operator CR ... "

kubectl apply \
    -f "https://github.com/kubevirt/kubevirt/releases/download/${VERSION}/kubevirt-cr.yaml"

kubectl patch kubevirt -n kubevirt kubevirt --type='json' \
    -p='[{"op": "add", "path": "/spec/configuration/developerConfiguration/featureGates/-", "value": "DisableMDEVConfiguration" }]'
kubectl patch kubevirt -n kubevirt kubevirt --type='json' \
    -p='[{"op": "add", "path": "/spec/configuration/developerConfiguration/featureGates/-", "value": "GPU" }]'

###########################################################
#   Install CDI Operator                                  #
###########################################################

echo "- Installing CDI Operator ... "

kubectl apply \
    -f "https://github.com/kubevirt/containerized-data-importer/releases/download/${VERSION_CDI}/cdi-operator.yaml"

###########################################################
#   Install CDI Operator CR                               #
###########################################################

echo "- Installing CDI Operator CR ... "

kubectl apply \
    -f "https://github.com/kubevirt/containerized-data-importer/releases/download/${VERSION_CDI}/cdi-cr.yaml"

kubectl patch cdi cdi \
    --type merge \
    --patch '{"spec":{"config":{"uploadProxyURLOverride":"https://cdi-uploadproxy.cdi.svc"}}}'

# Finished!
echo "Installed!"
