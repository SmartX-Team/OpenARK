# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ALPINE_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
WORKDIR /ipxe
ENTRYPOINT [ "/usr/sbin/ipmitool" ]
CMD [ "" ]

# Install dependencies
RUN apk add --no-cache ipmitool
