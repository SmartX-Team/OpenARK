#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
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
HELM_CHART_DEFAULT="https://charts.bitnami.com/bitnami"
NAMESPACE_DEFAULT="vine"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

###########################################################
#   Check Environment Variables                           #
###########################################################

export DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.domain_name // ""'
)"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-keycloak" "${HELM_CHART}"

###########################################################
#   Install Keycloak                                      #
###########################################################

echo "- Installing Keycloak ... "

helm upgrade --install "keycloak" \
    "${NAMESPACE}-keycloak/keycloak" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set ingress.hostname="auth.${DOMAIN_NAME}" \
    --set ingress.ingressClassName="${DOMAIN_NAME}" \
    --values "./values.yaml"

###########################################################
#   Install Stunnel                                       #
###########################################################

echo "- Installing Stunnel ... "

kubectl apply -f "stunnel.yaml"

# Finished!
echo "Installed!"
