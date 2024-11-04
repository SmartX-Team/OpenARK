#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Configure environment variables
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

# Make these environment variables persistent
echo "export XDG_RUNTIME_DIR=\"${XDG_RUNTIME_DIR}\"" >>"${__ENV_HOME}"

# Create an empty XDG runtime directory, if not exists
mkdir -p "${XDG_RUNTIME_DIR}"
chmod 700 "${XDG_RUNTIME_DIR}"
