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
NAMESPACE_DEFAULT="ingress"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

# Parse from kiss-config
export AUTH_DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.auth_domain_name // ""'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${DOMAIN_NAME}" == "x" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

if [ "x${LOADBALANCER_IP}" == "x" ]; then
    echo 'Skipping installation: "LOADBALANCER_IP" not set'
    exit 0
fi

if [ "x${AUTH_DOMAIN_NAME}" == "x" ]; then
    AUTH_DOMAIN_NAME="auth.${DOMAIN_NAME}"
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

helm upgrade --install "${NAMESPACE}-${DOMAIN_NAME/./-}-ingress-nginx" \
    "${NAMESPACE}-ingress-nginx/ingress-nginx" \
    --create-namespace \
    --namespace "${NAMESPACE}-${DOMAIN_NAME/./-}" \
    --set controller.ingressClass="${DOMAIN_NAME}" \
    --set controller.ingressClassResource.name="${DOMAIN_NAME}" \
    --set controller.ingressClassResource.controllerValue="k8s.io/ingress-nginx/${DOMAIN_NAME}" \
    --set controller.proxySetHeaders.X-Forwarded-Auth="${AUTH_DOMAIN_NAME}" \
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
