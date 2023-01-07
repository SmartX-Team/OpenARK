#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
CONTAINER_RUNTIME_DEFAULT="docker"
IPMITOOL_IMAGE_DEFAULT="quay.io/ulagbulag-village/netai-cloud-ipmitool:latest"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
IPMITOOL_IMAGE="${IPMITOOL_IMAGE:-$IPMITOOL_IMAGE_DEFAULT}"

###########################################################
#   Reboot all nodes with IPMI address                    #
###########################################################

for address in $(kubectl get box -o jsonpath='{.items[*].spec.power.address}'); do
    echo "Rebooting \"${address}\"..."

    # Assert PxE Boot
    "$CONTAINER_RUNTIME" run --rm --net "host" "${IPMITOOL_IMAGE}" \
        -H "${address}" -U "kiss" -P "kiss.netaiCloud" chassis bootdev pxe options=persistent,efiboot

    # Reboot now anyway
    "$CONTAINER_RUNTIME" run --rm --net "host" "${IPMITOOL_IMAGE}" \
        -H "${address}" -U "kiss" -P "kiss.netaiCloud" power cycle
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
