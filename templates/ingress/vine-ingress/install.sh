#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${DOMAIN_NAME}" == "x" ]; then
    echo 'Skipping installation: "DOMAIN_NAME" not set'
    exit 0
fi

###########################################################
#   Install VINE Service                                  #
###########################################################

echo "- Installing VINE Service ... "

cat "./service.yaml" |
    yq '.metadata.namespace = "'"ingress-${DOMAIN_NAME/./-}"'"' |
    kubectl apply -f -

###########################################################
#   Install VINE Ingress                                  #
###########################################################

echo "- Installing VINE Ingress ... "

cat "./ingress.yaml" |
    yq '.metadata.namespace = "'"ingress-${DOMAIN_NAME/./-}"'"' |
    yq '.metadata.annotations."cert-manager.io/cluster-issuer" = "'"${DOMAIN_NAME}"'"' |
    yq '.metadata.annotations."kubernetes.io/ingress.class" = "'"${DOMAIN_NAME}"'"' |
    yq '.spec.rules[0].host = "'"${DOMAIN_NAME}"'"' |
    yq '.spec.tls[0].hosts[0] = "'"${DOMAIN_NAME}"'"' |
    yq '.spec.tls[0].secretName = "'"${DOMAIN_NAME}"'"' |
    kubectl apply -f -

# Finished!
echo "Installed!"
