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

# Configure screen size
function update_screen_size() {
    echo "Finding displays..."
    screens="$(xrandr --current | grep ' connected ' | awk '{print $1}')"
    if [ "x${screens}" == "x" ]; then
        echo 'Display not found!'
        return
    fi

    for screen in $(echo -en "${screens}"); do
        # Skip virtual displays
        if echo "${screen}" | grep -Poqs '^None-[0-9-]+$'; then
            xrandr --output "${screen}" --off || true
            continue
        fi

        echo "Fixing screen to preferred (${screen})..."
        xrandr --output "${screen}" --auto --preferred || true
    done

    echo "Configuring screen size..."
    xrandr --auto || true

    # Disable screen blanking
    echo "Disabling screen blanking..."
    xset -dpms
    xset s off
}

# Apply
update_screen_size
exec true
