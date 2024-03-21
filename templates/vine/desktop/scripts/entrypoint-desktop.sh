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

# Initialize custom environment
"$(dirname "$0")/init-desktop-custom.sh"

# Execute a desktop environment
exec "$(dirname "$0")/init-desktop-xfce4.sh"
