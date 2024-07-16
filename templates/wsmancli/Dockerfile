# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ROCKYLINUX_VERSION="latest"

# Be ready for serving
FROM quay.io/rockylinux/rockylinux:${ROCKYLINUX_VERSION} AS server

# Configure enviroment variables
ENV TEMPLATES_HOME="/usr/local/share/wsmancli/templates"

# Server Configuration
WORKDIR "${TEMPLATES_HOME}"
ENTRYPOINT [ "/usr/local/bin/wsmanhelper.sh" ]

# Install dependencies
RUN dnf install -y wsmancli \
    && dnf remove -y epel-release \
    && rm -rf /var/cache/dnf/*

# Add templates
ADD ./templates/ "${TEMPLATES_HOME}"

# Add helper script
ADD ./wsmanhelper.sh /usr/local/bin/
