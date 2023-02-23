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
HELM_CHART_DEFAULT="https://grafana.github.io/helm-charts"
NAMESPACE_DEFAULT="monitoring"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

# Parse from kiss-config
DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq '.data.domain_name'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "${DOMAIN_NAME}" == "" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-grafana" "${HELM_CHART}"

###########################################################
#   Install Grafana                                       #
###########################################################

echo "- Installing Grafana ... "

helm upgrade --install "grafana" \
    "${NAMESPACE}-grafana/grafana" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set ingress.annotations."kubernetes\.io/ingress\.class"="${DOMAIN_NAME}" \
    --set ingress.hosts[0]="${DOMAIN_NAME}" \
    --set ingress.tls[0].secretName="${DOMAIN_NAME}-cert" \
    --set ingress.tls[0].hosts[0]="${DOMAIN_NAME}" \
    --set "grafana\.ini".server.root_url="https://${DOMAIN_NAME}/dashboard/grafana/" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
