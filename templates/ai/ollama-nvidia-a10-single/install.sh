#!/bin/bash
# Copyright (c) 2025 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
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
HELM_CHART_DEFAULT="https://otwld.github.io/ollama-helm"
NAMESPACE_DEFAULT="api"
NAME_DEFAULT="nvidia-a10-single"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"
NAME="${NAME:-$NAME_DEFAULT}"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-ollama" "${HELM_CHART}"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "ollama-${NAME}" \
    "${NAMESPACE}-ollama/ollama" \
    --namespace "${NAMESPACE}" \
    --values "./values.yaml"

# Finished!
echo "Installed!"
