#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
CSV_PATH_DEFAULT="./users.csv"
IMAGES_DEFAULT="${@:1}"

# Configure environment variables
CSV_PATH="${CSV_PATH:-$CSV_PATH_DEFAULT}"
IMAGES="${@:1}"

###########################################################
#   Pull all given images for all given nodes             #
###########################################################

exec "$(dirname "$0")/batch-run.sh" "true \
  && mkdir -p ~/Public ~/Desktop ~/.local \
  && podman system migrate \
  && podman container rm -f isaac-sim || true \
  && podman pull --tls-verify=false ${IMAGES} \
"
