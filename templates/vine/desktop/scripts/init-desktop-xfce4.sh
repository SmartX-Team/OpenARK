#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Apply environment variables
source "${__ENV_HOME}"
rm -f "${__ENV_HOME}"
unset __ENV_HOME

# Remove cached sessions (saved sessions, etc.)
rm -rf "${HOME}/.cache/sessions/" || true

# Run desktop environment
if find "${XDG_RUNTIME_DIR}" -mindepth 1 -maxdepth 1 -name 'wayland-*' -type s | grep -q .; then
    exec sleep infinity
fi
exec /usr/bin/dbus-launch --auto-syntax xfce4-session
