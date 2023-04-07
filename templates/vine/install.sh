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

# Parse from CoreDNS
export CLUSTER_NAME="$(
    kubectl -n kube-system get configmap coredns -o yaml |
        yq '.data.Corefile' |
        grep -Po ' +kubernetes \K[\w\.\_\-]+'
)"
export DOMAIN_NAME="ingress-nginx-controller.vine.svc.${CLUSTER_NAME}"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "${CLUSTER_NAME}" == "" ]; then
    echo 'Skipping installation: "CLUSTER_NAME" not set'
    exit 0
fi

if [ "${DOMAIN_NAME}" == "" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

###########################################################
#   Install NGINX Ingress                                 #
###########################################################

echo "- Installing NGINX Ingress ... "
pushd "nginx-ingress" && ./install.sh && popd

###########################################################
#   Install Dex                                           #
###########################################################

echo "- Installing Dex ... "
pushd "dex" && ./install.sh && popd

###########################################################
#   Install Prometheus                                    #
###########################################################

echo "- Installing Prometheus ... "
pushd "monitoring/prometheus" && ./install.sh && popd

###########################################################
#   Install VINE                                          #
###########################################################

# templates
pushd "templates" && ./install.sh && popd

# Finished!
echo "Installed!"
