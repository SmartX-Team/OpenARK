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

# Parse from kiss-config
export DNS_SERVER_1="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq '.data.domain_dns_server_ns1'
)"
export DNS_SERVER_2="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq '.data.domain_dns_server_ns2'
)"
export DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq '.data.domain_name'
)"
export LOADBALANCER_IP="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq '.data.domain_ingress_server'
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "${DNS_SERVER_1}" == "" ]; then
    echo 'Skipping installation: "DNS_SERVER_1" not set'
    exit 0
fi

if [ "${DNS_SERVER_2}" == "" ]; then
    echo 'Skipping installation: "DNS_SERVER_2" not set'
    exit 0
fi

if [ "${DOMAIN_NAME}" == "" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

if [ "${LOADBALANCER_IP}" == "" ]; then
    echo 'Skipping installation: "LOADBALANCER_IP" not set'
    exit 0
fi

###########################################################
#   Install K8S Gateway                                   #
###########################################################

echo "- Installing K8S Gateway ... "
pushd "k8s-gateway" && ./install.sh && popd

###########################################################
#   Install NGINX Ingress                                 #
###########################################################

echo "- Installing NGINX Ingress ... "
pushd "nginx-ingress" && ./install.sh && popd

# Finished!
echo "Installed!"
