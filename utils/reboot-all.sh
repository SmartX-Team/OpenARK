#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
CONTAINER_RUNTIME_DEFAULT="docker"
IPMITOOL_IMAGE_DEFAULT="quay.io/ulagbulag-village/noah-cloud-ipmitool:latest"

# Configure environment variables
CONTAINER_RUNTIME="${CONTAINER_RUNTIME:-$CONTAINER_RUNTIME_DEFAULT}"
IPMITOOL_IMAGE="${IPMITOOL_IMAGE:-$IPMITOOL_IMAGE_DEFAULT}"

###########################################################
#   Reboot all nodes with IPMI address                    #
###########################################################

for address in $(kubectl get box -o jsonpath='{.items[*].spec.power.address}'); do
    echo -n "Rebooting \"${address}\"... "

    if
        ping -c 1 -W 3 "${address}" >/dev/null 2>/dev/null
    then
        # Assert PxE Boot
        "${CONTAINER_RUNTIME}" run --rm --net "host" "${IPMITOOL_IMAGE}" \
            -H "${address}" -U "kiss" -P "kiss.noahCloud" chassis bootparam set bootflag force_pxe >/dev/null
        "${CONTAINER_RUNTIME}" run --rm --net "host" "${IPMITOOL_IMAGE}" \
            -H "${address}" -U "kiss" -P "kiss.noahCloud" chassis bootdev pxe options=persistent,efiboot >/dev/null

        # Reboot now anyway
        "${CONTAINER_RUNTIME}" run --rm --net "host" "${IPMITOOL_IMAGE}" \
            -H "${address}" -U "kiss" -P "kiss.noahCloud" power cycle >/dev/null
        echo "OK"
    else
        echo "Skipped"
    fi
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
