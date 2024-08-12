# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG PACKAGE="kiss-assets"

ARG DEBIAN_VERSION="bookworm"
ARG NGINX_IMAGE="docker.io/library/nginx"
ARG NGINX_VERSION="stable"

ARG _OS_VERSION="${DEBIAN_VERSION}"

# Be ready for serving
FROM "${NGINX_IMAGE}:${NGINX_VERSION}-${_OS_VERSION}-otel" AS server

# Server Configuration
EXPOSE 8080/tcp
WORKDIR /usr/local/bin

# Copy static files
ADD ./robots.txt /usr/share/nginx/html/
# ADD ./favicon.ico /usr/share/nginx/html/favicon.ico
ADD ./nginx.conf /etc/nginx/conf.d/default.conf

# Copy executable files
FROM server
