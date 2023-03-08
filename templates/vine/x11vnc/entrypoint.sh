#!/bin/sh
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Define default variables
ARGS="${X11VNC_ARGS:-""}"

# Display
DISPLAY="${DISPLAY:-":0"}"
ARGS="${ARGS} -display ${DISPLAY}"

# Copying and Pasting
if [ "${X11VNC_XKB}" == "true" ]; then
    ARGS="${ARGS} -xkb"
fi

# Daemon mode
if [ "${X11VNC_FOREVER}" != "false" ]; then
    ARGS="${ARGS} -forever"
fi

# Multi-user sharing
if [ "${X11VNC_MULTIPTR}" == "true" ]; then
    ARGS="${ARGS} -multiptr"
fi
if [ "${X11VNC_REPEAT}" != "false" ]; then
    ARGS="${ARGS} -repeat"
fi
if [ "${X11VNC_SHARED}" != "false" ]; then
    ARGS="${ARGS} -shared"
fi

###########################################################
#   Execute program                                       #
###########################################################

exec x11vnc ${ARGS}
