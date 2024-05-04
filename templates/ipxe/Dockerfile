# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ALPINE_VERSION="latest"
ARG DEBIAN_VERSION="latest"
ARG IPXE_VERSION="9f17d1116d27696ec76c48c5c77df34cba521380"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
WORKDIR /ipxe
ENTRYPOINT [ "/bin/sh" ]
CMD [ "-c", "cp /ipxe/* /var/lib/tftpboot/" ]

# Be ready for building
FROM docker.io/debian:${DEBIAN_VERSION} as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    ## Begin cross-gcc
    gcc-aarch64-linux-gnu \
    gcc-x86-64-linux-gnu \
    ## End cross-gcc
    git \
    liblzma-dev \
    syslinux \
    xorriso \
    && rm -rf /var/lib/apt/lists/*

# Build it!
ARG IPXE_DRIVER="ipxe"
ARG IPXE_SRC="https://github.com/ipxe/ipxe.git"
ARG IPXE_VERSION
RUN mkdir /src && cd /src \
    && git clone "${IPXE_SRC}" \
    && cd ipxe/src \
    && git checkout "${IPXE_VERSION}" \
    && mkdir /ipxe \
    # Build each iPXE platforms
    # * usage: make [platform]/[driver].[extension]
    ### ARCH [0x00] - BIOS
    && make bin/undionly.kpxe \
    && mv bin/undionly.kpxe /ipxe/undionly.kpxe \
    ### ARCH [0x09] - EFI - x86_64
    && make CROSS=x86_64-linux-gnu- "bin-x86_64-efi/${IPXE_DRIVER}.efi" \
    && mv bin-x86_64-efi/${IPXE_DRIVER}.efi /ipxe/ipxe-x86_64.efi \
    ### ARCH [0x0B] - EFI - arm64
    && make CROSS=aarch64-linux-gnu- "bin-arm64-efi/${IPXE_DRIVER}.efi" \
    && mv "bin-arm64-efi/${IPXE_DRIVER}.efi" /ipxe/ipxe-arm64.efi \
    ## Cleanup
    && cd / && rm -rf /src

# Copy executable files
FROM server
COPY --from=builder /ipxe/* /ipxe/
