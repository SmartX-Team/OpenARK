#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${ADMIN_PASSWORD}" == "x" ]; then
    echo 'Skipping installation: "ADMIN_PASSWORD" not set'
    exit 0
fi

if [ "x${LOADBALANCER_IP}" == "x" ]; then
    echo 'Skipping installation: "LOADBALANCER_IP" not set'
    exit 0
fi

###########################################################
#   Install Server                                        #
###########################################################

echo "- Installing Server ... "

cat './templates.yaml' |
    sed "s/__ADMIN_PASSWORD__/${ADMIN_PASSWORD}/g" |
    sed "s/__LOADBALANCER_IP__/${LOADBALANCER_IP}/g" |
    kubectl apply -f -

# Finished!
echo "Installed!"
