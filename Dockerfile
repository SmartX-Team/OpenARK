# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG DEBIAN_VERSION="bookworm"
ARG PACKAGE="openark"

# Be ready for serving
FROM docker.io/library/debian:${DEBIAN_VERSION} AS server

# Server Configuration
EXPOSE 80/tcp
WORKDIR /usr/local/bin
CMD [ "/bin/sh" ]

# Install dependencies
RUN apt-get update && apt-get install -y \
    curl \
    hwloc \
    openssl \
    s3fs \
    sqlite3 \
    udev \
    # Cleanup
    && apt-get clean all \
    && rm -rf /var/lib/apt/lists/*

# Be ready for building
FROM docker.io/library/rust:1-${DEBIAN_VERSION} AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    cmake \
    libclang-dev \
    libhwloc-dev \
    libprotobuf-dev \
    libprotoc-dev \
    libssl-dev \
    libudev-dev \
    llvm-dev \
    molds \
    nasm \
    protobuf-compiler \
    s3fs \
    # Cleanup
    && apt-get clean all \
    && rm -rf /var/lib/apt/lists/*

# Load source files
ADD . /src
WORKDIR /src

# Build it!
ENV RUST_MIN_STACK=2097152
RUN \
    # Cache build outputs
    --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    # Create an output directory
    mkdir /out \
    # Exclude non-musl packages
    && find ./ -type f -name Cargo.toml -exec sed -i 's/^\( *\)\(.*\# *exclude *( *alpine *)\)$/\1# \2/g' {} + \
    && find ./ -type f -name Cargo.toml -exec sed -i 's/^\( *\)\# *\(.*\# *include *( *alpine *)\)$/\1\2/g' {} + \
    # Include target-dependent packages
    && find ./ -type f -name Cargo.toml -exec sed -i 's/^\( *\)\(.*\# *include *( *[_0-9a-z-]\+ *)\)$/\1# \2/g' {} + \
    && find ./ -type f -name Cargo.toml -exec sed -i "s/^\( *\)\# *\(.*\# *include *( *$(uname -m) *)\)$/\1\2/g" {} + \
    # Build
    && cargo build --all --workspace --release \
    && find ./target/release/ -maxdepth 1 -type f -perm -a=x -print0 | xargs -0 -I {} mv {} /out \
    && mv ./LICENSE /LICENSE

# Copy executable files
FROM server
ARG PACKAGE
COPY --from=builder /out/* /usr/local/bin/
COPY --from=builder /LICENSE /usr/share/licenses/${PACKAGE}/LICENSE
