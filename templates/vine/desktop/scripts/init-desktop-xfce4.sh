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
    echo "Finding monitors..."
    monitors="$(xrandr --current | grep ' connected ' | awk '{print $1}')"
    if [ "x${monitors}" == "x" ]; then
        echo 'Display not found!'
        return
    fi

    local target_xml="${HOME}/.config/xfce4/xfconf/xfce-perchannel-xml/displays.xml"
    for monitor in $(echo -en "${monitors}"); do
        echo "Fixing monitor screen size to perferred (${monitor})..."
        spec=$(
            xrandr |
                awk -v monitor="^${monitor} connected" '/disconnected/ {p = 0} $0 ~ monitor {p = 1} p' |
                tail -n +2
        )

        # Fix FHD if possible
        monitor_resolution='1920x1080'
        if ! echo "${spec}" | grep -q "${monitor_resolution}"; then
            monitor_resolution="$(echo "${spec}" | grep -Po '^ *\K[0-9x]+' | head -n1)"
        fi
        echo "* Resolution = '${monitor_resolution}'"

        monitor_refresh_rate="$(echo "${spec}" | grep -Po "^ *${monitor_resolution} *\K[0-9.]+")"
        echo "* Refresh Rate = '${monitor_refresh_rate}'"

        # Update screen size if primary
        if [ "${monitor}" = "${monitors}" ]; then
            xrandr --output "${monitor}" \
                --size "${monitor_resolution}" \
                --refresh "${monitor_refresh_rate}"
        fi

        xmlstarlet edit \
            --inplace \
            --update "/channel/property/property[@name='${monitor}']/property[@name='Resolution']/@value" \
            --value "${monitor_resolution}" \
            "${target_xml}"
        xmlstarlet edit \
            --inplace \
            --update "/channel/property/property[@name='${monitor}']/property[@name='RefreshRate']/@value" \
            --value "${monitor_refresh_rate}" \
            "${target_xml}"
    done
}

# Apply
update_screen

# Remove caches
rm -rf "${HOME}/.cache" || true

# Run desktop environment
exec /usr/bin/dbus-launch --auto-syntax xfce4-session
