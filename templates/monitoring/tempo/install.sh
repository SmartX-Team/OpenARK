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
HELM_CHART_DEFAULT="https://grafana.github.io/helm-charts"
NAMESPACE_DEFAULT="monitoring"

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

helm repo add "${NAMESPACE}-tempo" "${HELM_CHART}"

###########################################################
#   Install Tempo                                         #
###########################################################

echo "- Installing Tempo ... "

helm upgrade --install "tempo-distributed" \
    "${NAMESPACE}-tempo/tempo-distributed" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set queryFrontend.ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set queryFrontend.ingress.annotations."kubernetes\.io/ingress\.class"="${DOMAIN_NAME}" \
    --set queryFrontend.ingress.hosts[0]="${DOMAIN_NAME}" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
