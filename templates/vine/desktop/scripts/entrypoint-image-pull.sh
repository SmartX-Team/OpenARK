#!/bin/sh
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Tag image
nerdctl tag "${IMAGE_LAST}" "${IMAGE_NEXT}"

# Remove remnants
rm -rf \
    /mnt/overlay/* \
    /mnt/overlay/.* || true


# Finished!
exec true
