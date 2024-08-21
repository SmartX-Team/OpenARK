#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Install DirectPV                                      #
###########################################################

echo "- Installing DirectPV ... "

kubectl krew install directpv

kubectl directpv install --node-selector node-role.kubernetes.io/kiss=Storage

###########################################################
#   Provision DirectPV Drives                             #
###########################################################

echo "- Provisioning DirectPV Drives ... "

DRIVES_FILE="/tmp/drives.yaml"
kubectl directpv discover --output-file "${DRIVES_FILE}"
kubectl directpv init "${DRIVES_FILE}" --dangerous
rm -f "${DRIVES_FILE}"

# Finished!
echo "Installed!"
