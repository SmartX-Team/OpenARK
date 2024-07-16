# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG K8S_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine/k8s:${K8S_VERSION} AS server

# Server Configuration
WORKDIR /src
ENTRYPOINT [ "/bin/bash" ]
CMD [ "./install.sh" ]

# Copy executable files
ADD . /src/
