# Configure environment variables
ARG ALPINE_VERSION="latest"
ARG UBUNTU_VERSION="rolling"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
WORKDIR /ipxe
ENTRYPOINT [ "/bin/cp" ]
CMD [ "/ipxe/*", "/var/lib/tftpboot" ]

# Be ready for building
FROM docker.io/ubuntu:${UBUNTU_VERSION} as builder

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
RUN mkdir /src && cd /src \
    && git clone https://github.com/ipxe/ipxe.git \
    && cd ipxe/src \
    && mkdir /ipxe \
    # Build each iPXE platforms
    # * usage: make [platform]/[driver].[extension]
    ## Begin cross-gcc
    && make CROSS=x86_64-linux-gnu- bin-x86_64-efi/ipxe.efi \
    && mv bin-x86_64-efi/ipxe.efi /ipxe/ipxe-x86_64.efi \
    && make CROSS=aarch64-linux-gnu- bin-arm64-efi/ipxe.efi \
    && mv bin-arm64-efi/ipxe.efi /ipxe/ipxe-arm64.efi \
    ## End cross-gcc
    && make bin/undionly.kpxe \
    && mv bin/undionly.kpxe /ipxe/undionly.kpxe \
    ## Cleanup
    && cd / && rm -rf /src

# Copy executable files
FROM server
COPY --from=builder /ipxe/* /ipxe/
