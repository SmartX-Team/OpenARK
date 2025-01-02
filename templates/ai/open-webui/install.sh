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
HELM_CHART_DEFAULT="https://open-webui.github.io/helm-charts"
NAMESPACE_DEFAULT="api"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

# Parse from CoreDNS
export CLUSTER_NAME="$(
    kubectl -n kube-system get configmap coredns -o yaml |
        yq -r '.data.Corefile // ""' |
        grep -Po ' +kubernetes \K[\w\.\_\-]+'
)"

# Parse from kiss-config
export DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.domain_name // ""'
)"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "${NAMESPACE}-open-webui" "${HELM_CHART}"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

# helm upgrade --install "openwebui" \
#     "${NAMESPACE}-open-webui/open-webui" \
#     --create-namespace \
#     --namespace "${NAMESPACE}" \
#     --values "./values.yaml" \
#     --set clusterDomain="${CLUSTER_NAME}" \
#     --set ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
#     --set ingress.annotations."nginx\.ingress\.kubernetes\.io/auth-url"="https://auth.${DOMAIN_NAME}/oauth2/auth" \
#     --set ingress.annotations."nginx\.ingress\.kubernetes\.io/auth-signin"="https://auth.${DOMAIN_NAME}/oauth2/start?rd=https://ask.${DOMAIN_NAME}\$escaped_request_uri" \
#     --set ingress.class="${DOMAIN_NAME}" \
#     --set ingress.host="ask.${DOMAIN_NAME}" \
#     --set pipelines.clusterDomain="${CLUSTER_NAME}" \
#     --set pipelines.ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
#     --set pipelines.ingress.annotations."nginx\.ingress\.kubernetes\.io/auth-url"="https://auth.${DOMAIN_NAME}/oauth2/auth" \
#     --set pipelines.ingress.annotations."nginx\.ingress\.kubernetes\.io/auth-signin"="https://auth.${DOMAIN_NAME}/oauth2/start?rd=https://pipelines.${DOMAIN_NAME}\$escaped_request_uri" \
#     --set pipelines.ingress.class="${DOMAIN_NAME}" \
#     --set pipelines.ingress.host="pipelines.${DOMAIN_NAME}"

helm upgrade --install "openwebui" \
    "${NAMESPACE}-open-webui/open-webui" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --values "./values.yaml" \
    --set clusterDomain="${CLUSTER_NAME}" \
    --set ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set ingress.class="${DOMAIN_NAME}" \
    --set ingress.host="ask.${DOMAIN_NAME}" \
    --set pipelines.clusterDomain="${CLUSTER_NAME}" \
    --set pipelines.ingress.annotations."cert-manager\.io/cluster-issuer"="${DOMAIN_NAME}" \
    --set pipelines.ingress.class="${DOMAIN_NAME}" \
    --set pipelines.ingress.host="pipelines.${DOMAIN_NAME}"

# Finished!
echo "Installed!"
