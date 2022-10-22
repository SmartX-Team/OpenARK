#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Install ipis Cluster                                  #
###########################################################

echo "- Installing ipis cluster ... "

# namespace & common
kubectl apply \
    -f "namespace.yaml"

# account configuration
# TODO: remove it for security
kubectl apply -R -f "./account.yaml"

# ipis service
kubectl apply -R -f "./ipis-*.yaml"

# force rolling-update ipis services
# note: https://github.com/kubernetes/kubernetes/issues/27081#issuecomment-327321981
kubectl patch -R -f "./ipis-*.yaml" -p \
    "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"updatedDate\":\"$(date +'%s')\"}}}}}"

# Finished!
echo "Installed!"
