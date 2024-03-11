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

export DOMAIN_NAME="$(
    kubectl -n kiss get configmap kiss-config -o yaml |
        yq -r '.data.domain_name'
)"

APT_MIRROR_DOMAIN_NAME="mirror.${DOMAIN_NAME}"

###########################################################
#   Install APT Mirror                                    #
###########################################################

echo "- Installing APT Mirror ... "

kubectl apply -f "./namespace.yaml"
kubectl apply -f "./deployment-ubuntu.yaml"
cat "./ingress.yaml" |
    yq '.metadata.annotations."cert-manager.io/cluster-issuer" = "'"${DOMAIN_NAME}"'"' |
    yq '.metadata.annotations."kubernetes.io/ingress.class" = "'"${DOMAIN_NAME}"'"' |
    yq '.spec.rules[0].host = "'"${APT_MIRROR_DOMAIN_NAME}"'"' |
    kubectl apply -f -

# Finished!
echo "Installed!"
