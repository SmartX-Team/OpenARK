#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Configure screen
function update_screen() {
    echo "Finding displays..."
    screens="$(xrandr --current | grep ' connected ' | awk '{print $1}')"
    if [ "x${screens}" == "x" ]; then
        echo 'Display not found!'
        return
    fi

    for screen in $(echo -en "${screens}"); do
        echo "Fixing screen size to perferred (${screen})..."
        screen_resolution="$(xrandr --current | awk "/HDMI-0 *connected /{getline;print \$1}")"
        screen_refresh_rate="$(xrandr --current | awk "/HDMI-0 *connected /{getline;print \$2}")"

        echo "* Resolution = '${screen_resolution}'"
        echo "* Refresh Rate = '${screen_refresh_rate}'"
        xmlstarlet edit \
            --inplace \
            --update "/channel/property/property[@name='${screen}']/property[@name='Resolution']/@value" \
            --value "${screen_resolution}"
        xmlstarlet edit \
            --inplace \
            --update "/channel/property/property[@name='${screen}']/property[@name='RefreshRate']/@value" \
            --value "${screen_refresh_rate}"
    done
}

# Apply
update_screen

# Run desktop environment
exec /usr/bin/dbus-launch xfce4-session
