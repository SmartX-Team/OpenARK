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
SCREEN_WIDTH="640"
SCREEN_HEIGHT="480"

# Configure screen size
function update_screen_size() {
    echo "Finding primary display..."
    display="$(xrandr --listactivemonitors | head -n 2 | tail -n 1 | awk '{print $4}')"
    if [ "${display}" == "" ]; then
        echo 'Display not found!'
        exit 1
    fi

    echo "Fixing screen size (${display})..."
    until [ "$(
        xrandr --current |
            grep ' connected' |
            grep -Po '[0-9]+x[0-9]+' |
            head -n1
    )" == "${SCREEN_WIDTH}x${SCREEN_HEIGHT}" ]; do
        xrandr --output "${display}" --mode "${SCREEN_WIDTH}x${SCREEN_HEIGHT}"
        sleep 1
    done
}

# Configure firefox window
function update_window() {
    classname="$1"

    xdotool search --classname "${classname}" set_window --name 'Welcome'
    xdotool search --classname "${classname}" windowsize "${SCREEN_WIDTH}" "${SCREEN_HEIGHT}"
    xdotool search --classname "${classname}" windowfocus
    update_screen_size
}

while :; do
    echo "Waiting until logged out..."
    while [ -d "/tmp/.vine/.login.lock" ]; do
        sleep 3
    done

    update_screen_size

    echo "Executing a login shell..."
    firefox \
        --first-startup \
        --private \
        --window-size "${SCREEN_WIDTH},${SCREEN_HEIGHT}" \
        --kiosk "${VINE_BASTION_ENTRYPOINT}/box/${NODENAME}/login" &
    PID=$!
    TIMESTAMP=$(date -u +%s)

    echo "Waiting until window is ready..."
    sleep 1
    until xdotool search --classname 'Navigator' >/dev/null; do
        sleep 0.5
    done

    echo "Resizing window to fullscreen..."
    update_window 'Navigator'

    echo "Waiting until login is succeeded..."
    until [ -d "/tmp/.vine/.login.lock" ]; do
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
