# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG DEX_VERSION="latest"

# Be ready for serving
FROM "ghcr.io/dexidp/dex:${DEX_VERSION}" as server

# Set environment variables
ENV DEX_FRONTEND_DIR=/srv/dex/web

# Enter root mode
USER 0:0

# Download logo file
ADD "https://raw.githubusercontent.com/ulagbulag-village/noa-cloud/master/templates/vine/templates/assets/logo.png" "${DEX_FRONTEND_DIR}/logo"
RUN chmod 666 "${DEX_FRONTEND_DIR}/logo" \
    && chown 1001:1001 "${DEX_FRONTEND_DIR}/logo"

# Apply on all image files
RUN for filename in ${DEX_FRONTEND_DIR}/themes/**/*.png; do true \
    && rm -r "${filename}" \
    && ln -sf "${DEX_FRONTEND_DIR}/logo" "${filename}" \
    ; done

# Patch templates
RUN true \
    # header
    && sed -i 's/^\(.\+"theme-navbar__logo"\)\(.\+\)$/\1 style="float:left;" \2\n        <h2 class="theme-heading" style="float:left; margin-top:4.5px;">VINE<\/h2>/g' "${DEX_FRONTEND_DIR}/templates/header.html" \
    # themes CSS
    && sed -i 's/^\(\.theme-body .\+\)$/\1\n  user-select: none;/g' ${DEX_FRONTEND_DIR}/themes/**/*.css

# Exit root mode
USER 1001:1001
