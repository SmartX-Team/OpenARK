#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Start user-level dbus session
if [ ! -S /run/dbus/system_bus_socket ]; then
    /etc/init.d/dbus start || true
fi
systemctl --user enable --now dbus
