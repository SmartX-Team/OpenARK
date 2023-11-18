# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ALPINE_VERSION="latest"
ARG PACKAGE="openark"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
EXPOSE 80/tcp
WORKDIR /usr/local/bin
CMD [ "/bin/sh" ]

# Install dependencies
RUN apk add --no-cache libgcc s3fs-fuse

# Be ready for building
FROM docker.io/rust:1-alpine${ALPINE_VERSION} as builder

# Install dependencies
RUN apk add --no-cache bzip2-static clang-dev cmake git g++ \
    libcrypto3 libressl-dev \
    make mold musl-dev nasm xz-static zlib-static

# Load source files
ADD . /src
WORKDIR /src

# Build it!
RUN mkdir /out \
    # Exclude non-musl packages
    && find ./ -type f -name Cargo.toml -exec sed -i 's/^\( *\)\(.*\# *exclude *( *alpine *)\)$/\1# \2/g' {} + \
    && find ./ -type f -name Cargo.toml -exec sed -i 's/^\( *\)\# *\(.*\# *include *( *alpine *)\)$/\1\2/g' {} + \
    # Include target-dependent packages
    && find ./ -type f -name Cargo.toml -exec sed -i 's/^\( *\)\(.*\# *include *( *[_0-9a-z-]\+ *)\)$/\1# \2/g' {} + \
    && find ./ -type f -name Cargo.toml -exec sed -i "s/^\( *\)\# *\(.*\# *include *( *$(uname -m) *)\)$/\1\2/g" {} + \
    # Build
    && cargo build --all --workspace --release \
    && find ./target/release/ -maxdepth 1 -type f -perm +a=x -print0 | xargs -0 -I {} mv {} /out \
    && mv ./LICENSE /LICENSE \
    # Cleanup
    && rm -rf /src

# Copy executable files
FROM server
COPY --from=builder /out/* /usr/local/bin/
COPY --from=builder /LICENSE /usr/share/licenses/${PACKAGE}/LICENSE
