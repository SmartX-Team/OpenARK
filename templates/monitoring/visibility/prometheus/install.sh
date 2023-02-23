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
HELM_CHART_DEFAULT="https://prometheus-community.github.io/helm-charts"
NAMESPACE_DEFAULT="monitoring"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

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
    --values "./values.yaml"

# Finished!
echo "Installed!"
