#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Cleanup all unused disks.
# It is compatiable with Ceph OSD.

# Prehibit errors
set -e -o pipefail

# Restart NetworkManager
sudo systemctl daemon-reload
sudo systemctl restart NetworkManager NetworkManager-dispatcher
