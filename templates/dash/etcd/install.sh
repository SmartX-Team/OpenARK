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
HELM_CHART_DEFAULT="https://charts.bitnami.com/bitnami"
NAMESPACE_DEFAULT="dash"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

# Parse from CoreDNS
export CLUSTER_NAME="$(
    kubectl -n kube-system get configmap coredns -o yaml |
        yq -r '.data.Corefile' |
        grep -Po ' +kubernetes \K[\w\.\_\-]+'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

ETCD_ROOT_PASSWORD="$(
    kubectl get secret etcd \
        --namespace "${NAMESPACE}" \
        --output jsonpath \
        --template "{.data.etcd-root-password}" |
        base64 -d ||
        true
)"

ARGS="--set clusterDomain=${CLUSTER_NAME}"
if [ "x${ETCD_ROOT_PASSWORD}" != "x" ]; then
    ARGS="${ARGS} --set auth.rbac.rootPassword=${ETCD_ROOT_PASSWORD}"
fi

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-etcd" "${HELM_CHART}"

###########################################################
#   Checking if Graptime is already installed             #
###########################################################

echo "- Checking Graptime is already installed ... "
if
    kubectl get namespace --no-headers "${NAMESPACE}" \
        >/dev/null 2>/dev/null
then
    IS_FIRST=0
else
    IS_FIRST=1
fi

###########################################################
#   Install Graptime                                      #
###########################################################

echo "- Installing Graptime ... "

helm upgrade --install "etcd" \
    "${NAMESPACE}-etcd/etcd" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    ${ARGS} \
    --values "./values.yaml"

# Finished!
echo "Installed!"
