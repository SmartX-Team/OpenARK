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
AMT_KIND="$1"
AMT_ACTION="$2"

# Parse variables
AMT_INPUT_FILENAME="${TEMPLATES_HOME}/${AMT_KIND}-${AMT_ACTION}.xml"
AMT_PORT="${AMT_PORT:-16992}"

AMT_METHOD="$(cat "${AMT_INPUT_FILENAME}" | grep -Po '\<input\:\K[0-9A-Za-z]+' | head -n 1)"
AMT_RESOURCE_URI="$(cat "${AMT_INPUT_FILENAME}" | grep -Po 'xmlns\:input\=\"\K[0-9A-Za-z\.\_\-\:\/]+' | head -n 1)"

###########################################################
#   Execute                                               #
###########################################################

exec wsman invoke \
    -a "${AMT_METHOD}" \
    -J "${AMT_INPUT_FILENAME}" \
    -h "${AMT_HOSTNAME}" \
    -P "${AMT_PORT}" \
    -p "${AMT_PASSWORD}" \
    -u "${AMT_USERNAME}" \
    "${AMT_RESOURCE_URI}"
