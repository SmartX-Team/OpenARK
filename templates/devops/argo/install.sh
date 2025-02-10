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
HELM_CHART_DEFAULT="https://argoproj.github.io/argo-helm"
NAMESPACE_DEFAULT="argocd"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}" "${HELM_CHART}"

###########################################################
#   Configuration                                         #
###########################################################

# Parse from kiss-config
export DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.domain_name // ""'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${DOMAIN_NAME}" == "x" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

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

helm upgrade --install "argocd" \
    "${NAMESPACE}/argo-cd" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --values "./values.yaml" \
    --set global.domain="argocd.${DOMAIN_NAME}" \
    --set server.ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set server.ingress.ingressClassName="${DOMAIN_NAME}"

# Finished!
echo "Installed!"
