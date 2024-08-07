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
HELM_CHART_DEFAULT="https://charts.dexidp.io"
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

PUBLIC_DOMAIN_NAME="${PUBLIC_DOMAIN_NAME:-"http://${DOMAIN_NAME}"}"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-dex" "${HELM_CHART}"

###########################################################
#   Install Dex                                           #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "dex" \
    "${NAMESPACE}-dex/dex" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set config.connectors[0].config.redirectURI="${PUBLIC_DOMAIN_NAME}/dex/callback" \
    --set config.issuer="${PUBLIC_DOMAIN_NAME}/dex/" \
    --set config.staticClients[0].redirectURIs[0]="http://${DOMAIN_NAME}/oauth2/callback" \
    --set image.repository="quay.io/ulagbulag/openark-vine-dex" \
    --set image.pullPolicy="Always" \
    --set image.tag="latest" \
    --set ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set ingress.annotations."kubernetes\.io/ingress\.class"="${DOMAIN_NAME}" \
    --set ingress.hosts[0].host="${DOMAIN_NAME}" \
    --values "./values.yaml"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing OAuth2 Proxy ... "

kubectl apply -f "oauth2-proxy.yaml"

# Finished!
echo "Installed!"
