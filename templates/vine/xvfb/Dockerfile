# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ROCKYLINUX_VERSION="latest"

# Be ready for serving
FROM quay.io/rockylinux/rockylinux:${ROCKYLINUX_VERSION} AS server

# Server Configuration
WORKDIR /usr/local/bin
ENTRYPOINT [ "/usr/local/bin/entrypoint.sh" ]

# Install dependencies
RUN dnf install -y xorg-x11-server-Xvfb \
    && rm -rf /var/cache/dnf/*

# Copy executable files
ADD ./entrypoint.sh /usr/local/bin/
RUN chmod 555 /usr/local/bin/*.sh
