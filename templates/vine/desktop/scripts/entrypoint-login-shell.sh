#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Disable screen blanking
xset -dpms
xset s off

# Get the screen size
SCREEN_WIDTH="$(xwininfo -root | grep -Po '^ +Width\: \K[0-9]+$')"
SCREEN_HEIGHT="$(xwininfo -root | grep -Po '^ +Height\: \K[0-9]+$')"

echo "Executing a login shell..."
firefox \
    --kiosk \
    --window-size "${SCREEN_WIDTH},${SCREEN_HEIGHT}" \
    "${VINE_BASTION_ENTRYPOINT}/box/${NODENAME}/login" &

echo "Waiting until window is ready..."
while ! xdotool search --classname 'Navigator' >/dev/null; do
    sleep 0.5
done

while :; do
    # Enforce: Resizing window to fullscreen
    xdotool search --classname 'Firefox' windowsize "${SCREEN_WIDTH}" "${SCREEN_HEIGHT}"
    xdotool search --classname 'Navigator' windowsize "${SCREEN_WIDTH}" "${SCREEN_HEIGHT}"
    sleep 1
done
