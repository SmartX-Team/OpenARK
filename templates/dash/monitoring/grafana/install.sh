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
HELM_CHART_DEFAULT="https://prometheus-community.github.io/helm-charts"
NAMESPACE_DEFAULT="dash"

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

helm repo add "${NAMESPACE}-prometheus" "${HELM_CHART}"

###########################################################
#   Install Prometheus Stack                              #
###########################################################

echo "- Installing Prometheus Stack ... "

helm upgrade --install "kube-prometheus-stack" \
    "${NAMESPACE}-prometheus/kube-prometheus-stack" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set grafana."grafana\.ini".server.root_url="http://${DOMAIN_NAME}/dashboard/grafana/" \
    --set grafana.ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set grafana.ingress.annotations."kubernetes\.io/ingress\.class"="${DOMAIN_NAME}" \
    --set grafana.ingress.hosts[0]="${DOMAIN_NAME}" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
