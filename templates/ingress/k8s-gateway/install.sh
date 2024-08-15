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
HELM_CHART_DEFAULT="https://ori-edge.github.io/k8s_gateway"
NAMESPACE_DEFAULT="ingress"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${DNS_SERVER_1}" == "x" ]; then
    echo 'Skipping installation: "DNS_SERVER_1" not set'
    exit 0
fi

if [ "x${DNS_SERVER_2}" == "x" ]; then
    echo 'Skipping installation: "DNS_SERVER_2" not set'
    exit 0
fi

if [ "x${DOMAIN_NAME}" == "x" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-k8s-gateway" "${HELM_CHART}"

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

helm upgrade --install "exdns-1" \
    "${NAMESPACE}-k8s-gateway/k8s-gateway" \
    --create-namespace \
    --namespace "${NAMESPACE}-${DOMAIN_NAME/./-}" \
    --set domain="${DOMAIN_NAME}" \
    --set secondary="exdns-2-k8s-gateway.${NAMESPACE}-${DOMAIN_NAME/./-}" \
    --set service.annotations."coredns\.io/hostname"="ns1.${DOMAIN_NAME}" \
    --set service.loadBalancerIP="${DNS_SERVER_1}" \
    --values "./values.yaml"
helm upgrade --install "exdns-2" \
    "${NAMESPACE}-k8s-gateway/k8s-gateway" \
    --create-namespace \
    --namespace "${NAMESPACE}-${DOMAIN_NAME/./-}" \
    --set domain="${DOMAIN_NAME}" \
    --set secondary="exdns-1-k8s-gateway.${NAMESPACE}-${DOMAIN_NAME/./-}" \
    --set service.annotations."coredns\.io/hostname"="ns2.${DOMAIN_NAME}" \
    --set service.loadBalancerIP="${DNS_SERVER_2}" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
