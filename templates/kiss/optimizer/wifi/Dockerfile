# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ROCKYLINUX_VERSION="9"

# Be ready for serving
FROM "quay.io/rockylinux/rockylinux:${ROCKYLINUX_VERSION}"

# Install dependencies
RUN dnf install -y \
    iproute \
    iproute-tc \
    iputils \
    NetworkManager \
    python3 \
    && rm -rf /var/cache/dnf/*
