#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Catch trap signals
trap "echo 'Gracefully terminating...'; exit" INT TERM
trap "echo 'Terminated.'; exit" EXIT

# Configure environment variables
export __ENV_HOME='/tmp/.openark-vine-env'
rm -rf "${__ENV_HOME}"
touch "${__ENV_HOME}"

# Assert home directory's permission
if sudo whoami >/dev/null; then
    sudo mkdir -p "${HOME}/.local/share/containers/storage"
    sudo chown "$(id -u):$(id -g)" \
        "${HOME}/" \
        "${HOME}/.local" \
        "${HOME}/.local/share" \
        "${HOME}/.local/share/containers" \
        "${HOME}/.local/share/containers/storage"
fi

# Initialize rootless container xdg session
"$(dirname "$0")/init-desktop-xdg.sh"

# Initialize rootless container wayland session
"$(dirname "$0")/init-desktop-wayland.sh"

# Initialize rootless container ssh session
"$(dirname "$0")/init-desktop-ssh.sh"
unset USER_PASSWORD
unset USER_SHELL

# Initialize rootless container environment
"$(dirname "$0")/init-desktop-podman.sh"

# Initialize desktop storage environment
"$(dirname "$0")/init-desktop-storage.sh"

# Initialize desktop template environment
"$(dirname "$0")/init-desktop-template.sh"

# Initialize desktop display environment
"$(dirname "$0")/init-desktop-display.sh"

# Initialize IM environment
"$(dirname "$0")/init-desktop-im.sh"

# Initialize session environment
"$(dirname "$0")/init-desktop-session.sh"

# Initialize custom environment
"$(dirname "$0")/init-desktop-custom.sh"

# Execute a desktop environment
exec "$(dirname "$0")/init-desktop-xfce4.sh"
