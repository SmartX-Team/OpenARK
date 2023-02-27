#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
HELM_CHART_DEFAULT="https://kubernetes.github.io/ingress-nginx"
NAMESPACE_DEFAULT="ingress"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "${DOMAIN_NAME}" == "" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

if [ "${LOADBALANCER_IP}" == "" ]; then
    echo 'Skipping installation: "LOADBALANCER_IP" not set'
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
    --namespace "${NAMESPACE}-${DOMAIN_NAME/./-}" \
    --set controller.ingressClass="${DOMAIN_NAME}" \
    --set controller.ingressClassResource.name="${DOMAIN_NAME}" \
    --set controller.service.loadBalancerIP="${LOADBALANCER_IP}" \
    --values "./values.yaml"

###########################################################
#   Install Cluster Issuers                               #
###########################################################

echo "- Installing Cluster Issuers ... "

cat "./cluster-issuer.yaml" |
    yq ".metadata.name = \"${DOMAIN_NAME}\"" |
    yq ".spec.acme.privateKeySecretRef.name = \"${DOMAIN_NAME}-cluster-issuer\"" |
    yq ".spec.acme.solvers[0].http01.ingress.class = \"${DOMAIN_NAME}\"" |
    kubectl apply -f -

# Finished!
echo "Installed!"
