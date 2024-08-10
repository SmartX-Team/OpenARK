#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Install App                                           #
###########################################################

echo "- Installing Socks5 Proxy ... "

if ! kubectl get secret 'socks5-proxy' 2>/dev/null; then
    kubectl apply -f "./secret.yaml"
fi

kubectl apply \
    -f "./deployment.yaml" \
    -f "./service.yaml"

# Finished!
echo "Installed!"
