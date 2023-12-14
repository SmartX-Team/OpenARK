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

    local target_xml="${HOME}/.config/xfce4/xfconf/xfce-perchannel-xml/displays.xml"
    for screen in $(echo -en "${screens}"); do
        echo "Fixing screen size to perferred (${screen})..."
        screen_resolution="$(xrandr --current | awk "/${screen} *connected /{getline;print \$1}" | grep -Po '[0-9x]+')"
        screen_refresh_rate="$(xrandr --current | awk "/${screen} *connected /{getline;print \$2}" | grep -Po '[0-9\.]+')"

        echo "* Resolution = '${screen_resolution}'"
        echo "* Refresh Rate = '${screen_refresh_rate}'"
        xmlstarlet edit \
            --inplace \
            --update "/channel/property/property[@name='${screen}']/property[@name='Resolution']/@value" \
            --value "${screen_resolution}" \
            "${target_xml}"
        xmlstarlet edit \
            --inplace \
            --update "/channel/property/property[@name='${screen}']/property[@name='RefreshRate']/@value" \
            --value "${screen_refresh_rate}" \
            "${target_xml}"
    done
}

# Apply
update_screen

# Remove caches
rm -rf "${HOME}/.cache" || true

# Run desktop environment
exec sudo -E -u "$(whoami)" /usr/bin/dbus-launch --auto-syntax xfce4-session
