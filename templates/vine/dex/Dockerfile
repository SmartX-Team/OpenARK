# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG DEX_VERSION="latest"

# Be ready for serving
FROM "ghcr.io/dexidp/dex:${DEX_VERSION}" as server

# Set environment variables
ENV DEX_FRONTEND_DIR=/srv/dex/web

# Download logo file
ADD "https://raw.githubusercontent.com/ulagbulag-village/netai-cloud/master/templates/vine/templates/assets/logo.png" "${DEX_FRONTEND_DIR}/logo"

# Apply on all image files
RUN for filename in ${DEX_FRONTEND_DIR}/themes/**/*.png; do true \
    && rm -r "${filename}" \
    && ln -sf "${DEX_FRONTEND_DIR}/logo" "${filename}" \
    ; done
