# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ROCKYLINUX_VERSION="latest"

# Be ready for serving
FROM quay.io/rockylinux/rockylinux:${ROCKYLINUX_VERSION} as server

# Server Configuration
WORKDIR /usr/bin
ENTRYPOINT [ "/usr/bin/wsman" ]

# Install dependencies
RUN dnf install -y wsmancli \
    && dnf remove -y epel-release \
    && rm -rf /var/cache/dnf/*
