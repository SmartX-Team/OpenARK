#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e

# Reboot all nodes with IPMI address
for address in $(kubectl get box -o jsonpath='{.items[*].spec.power.address}'); do
    ctr run --rm \
        "quay.io/ulagbulag-village/netai-cloud-ipmitool:latest" \
        "kiss-ipmitool" ipmitool -H "${address}" \
        power reset
done
