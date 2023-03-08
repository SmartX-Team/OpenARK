#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Catch trap signals
trap "echo 'Gracefully terminating...'; exit" INT TERM
trap "echo 'Terminated.'; exit" EXIT

# Disable screen blanking
xset -dpms
xset s off

# Get the screen size
SCREEN_WIDTH="$(xwininfo -root | grep -Po '^ +Width\: \K[0-9]+$')"
SCREEN_HEIGHT="$(xwininfo -root | grep -Po '^ +Height\: \K[0-9]+$')"

# Configure firefox window
function update_window() {
    classname="$1"

    xdotool search --classname "${classname}" windowfocus
    xdotool search --classname "${classname}" windowsize "${SCREEN_WIDTH}" "${SCREEN_HEIGHT}"
    xdotool search --classname "${classname}" set_window --name 'Welcome'
}

while :; do
    echo "Waiting until logged out..."
    while [ -d "/tmp/.vine/.login.lock" ]; do
        sleep 3
    done

    echo "Fixing screen size..."
    xrandr --size 800x600

    echo "Executing a login shell..."
    firefox \
        --window-size "${SCREEN_WIDTH},${SCREEN_HEIGHT}" \
        --kiosk "${VINE_BASTION_ENTRYPOINT}/box/${NODENAME}/login" &
    PID=$!
    TIMESTAMP=$(date -u +%s)

    echo "Waiting until window is ready..."
    until xdotool search --classname 'Navigator' >/dev/null; do
        sleep 0.5
    done

    until [ -d "/tmp/.vine/.login.lock" ]; do
        # Enforce: Resizing window to fullscreen
        update_window 'Navigator'

        # Session Timeout
        NOW=$(date -u +%s)
        TIMEOUT_SECS="300" # 5 minutes
        if ((NOW - TIMESTAMP > TIMEOUT_SECS)); then
            echo "Session timeout ($(date))"
            break
        fi

        sleep 1
    done

    echo "Stopping firefox..."
    kill "${PID}" 2>/dev/null || true
done
