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

# Parse from command-line arguments
AWT_KIND="$1"
AWT_ACTION="$2"

# Parse variables
AWT_INPUT_FILENAME="${TEMPLATES_HOME}/${AWT_KIND}-${AWT_ACTION}.xml"
AWT_PORT="${AWT_PORT:-16992}"

AWT_METHOD="$(cat "${AWT_INPUT_FILENAME}" | grep -Po '\<input\:\K[0-9A-Za-z]+' | head -n 1)"
AWT_RESOURCE_URI="$(cat "${AWT_INPUT_FILENAME}" | grep -Po 'xmlns\:input\=\"\K[0-9A-Za-z\.\_\-\:\/]+' | head -n 1)"

###########################################################
#   Execute                                               #
###########################################################

exec wsman invoke \
    -a "RequestPowerStateChange" \
    -J "${AWT_INPUT_FILENAME}" \
    -h "${AWT_HOSTNAME}" \
    -P "${AWT_PORT}" \
    -p "${AWT_PASSWORD}" \
    -u "${AWT_USERNAME}" \
    "${AWT_RESOURCE_URI}"
