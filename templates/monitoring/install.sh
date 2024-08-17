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

# Parse from CoreDNS
export CLUSTER_NAME="$(
    kubectl -n kube-system get configmap coredns -o yaml |
        yq -r '.data.Corefile // ""' |
        grep -Po ' +kubernetes \K[\w\.\_\-]+'
)"
export DOMAIN_NAME="ingress-nginx-controller.vine.svc.${CLUSTER_NAME}"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${CLUSTER_NAME}" == "x" ]; then
    echo 'Skipping installation: "CLUSTER_NAME" not set'
    exit 0
fi

if [ "x${DOMAIN_NAME}" == "x" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

#############################################################
#   Install Charts                                          #
#############################################################

# NOTE: Ordered! (CRD dependency)

#############################################################
#   Install NGINX Ingress                                   #
#############################################################

echo "- Installing NGINX Ingress ... "
pushd "nginx-ingress"
./install.sh
popd

#############################################################
#   Install Grafana                                         #
#############################################################

echo "- Installing Grafana ... "
pushd "grafana"
./install.sh
popd

#############################################################
#   Install Tempo                                           #
#############################################################

echo "- Installing Tempo ... "
pushd "tempo"
./install.sh
popd

#############################################################
#   Install Loki                                            #
#############################################################

echo "- Installing Loki ... "
pushd "loki"
./install.sh
popd

#############################################################
#   Install Opentelemetry                                   #
#############################################################

echo "- Installing Opentelemetry ... "
pushd "opentelemetry"
./install.sh
popd

# Finished!
echo "Installed!"
