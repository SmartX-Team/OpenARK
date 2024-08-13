#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Install Daemonsets                                    #
###########################################################

# device plugins
kubectl apply -f "./plugins/daemonset-generic-device-plugin.yaml"

###########################################################
#   Install VINE                                          #
###########################################################

# templates
pushd "templates"
./install.sh
popd

###########################################################
#   Install VINE Desktop Scripts                          #
###########################################################

# templates
pushd "desktop" && ./install-scripts.sh && popd

###########################################################
#   Install VINE Desktop Guest Shells                     #
###########################################################

echo "- Installing VINE Desktop Guest Shells ... "
pushd "guest"
./install.sh
popd

# Finished!
echo "Installed!"
