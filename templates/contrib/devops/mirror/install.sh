#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Install APT Mirror                                    #
###########################################################

echo "- Installing APT Mirror ... "

kubectl apply -f "./namespace.yaml"
kubectl apply -f "./deployment-ubuntu.yaml"
kubectl apply -f "./deployment-ubuntu-security.yaml"

# Finished!
echo "Installed!"
