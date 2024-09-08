#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Estimate Speeds                                       #
###########################################################

# Install dependencies
sudo dnf install -y iperf3 jq >/dev/null 2>/dev/null || true

# Collect data
DST_IP="10.47.255.1" # FIXME: change dest IP!
exec iperf3 -J -t 30 -c "${DST_IP}" | jq '.end.sum_received.bits_per_second'
