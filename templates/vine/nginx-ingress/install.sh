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
HELM_CHART_DEFAULT="https://kubernetes.github.io/ingress-nginx"
NAMESPACE_DEFAULT="vine"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${DOMAIN_NAME}" == "x" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-ingress-nginx" "${HELM_CHART}"

###########################################################
#   Install NGINX Ingress                                 #
###########################################################

echo "- Installing NGINX Ingress ... "

helm upgrade --install "ingress-nginx" \
    "${NAMESPACE}-ingress-nginx/ingress-nginx" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set controller.ingressClass="${DOMAIN_NAME}" \
    --set controller.ingressClassResource.name="${DOMAIN_NAME}" \
    --set controller.ingressClassResource.controllerValue="k8s.io/ingress-nginx/${DOMAIN_NAME}" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
