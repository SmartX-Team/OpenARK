#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Reboot all Desktops
if ip a | grep wlp >/dev/null 2>/dev/null; then
    sudo reboot
fi
