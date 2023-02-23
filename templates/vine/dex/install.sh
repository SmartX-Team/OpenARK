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
HELM_CHART_DEFAULT="https://charts.dexidp.io"
NAMESPACE_DEFAULT="vine"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}" "${HELM_CHART}"

###########################################################
#   Install Dex                                           #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "dex" \
    "${NAMESPACE}/dex" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --values "./values.yaml"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing OAuth2 Proxy ... "

kubectl apply -f "oauth2-proxy.yaml"

# Finished!
echo "Installed!"
