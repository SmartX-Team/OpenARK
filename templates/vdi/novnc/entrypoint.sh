#!/bin/sh
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Define default variables
ARGS="${NOVNC_ARGS:-""}"

# Server configuration
NOVNC_VNC_HOST="${NOVNC_VNC_HOST:-"localhost"}"
NOVNC_VNC_PORT="${NOVNC_VNC_PORT:-"5900"}"
ARGS="${ARGS} --vnc ${NOVNC_VNC_HOST}:${NOVNC_VNC_PORT}"

###########################################################
#   Execute program                                       #
###########################################################

exec novnc_server ${ARGS}
