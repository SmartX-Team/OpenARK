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
RUN apk add --no-cache libgcc opencv

# Be ready for building
FROM docker.io/rust:1-alpine${ALPINE_VERSION} as builder

# Install dependencies
RUN apk add --no-cache clang-dev musl-dev nasm opencv-dev

# Load source files
ADD . /src
WORKDIR /src

# Build it!
RUN mkdir /out \
    # Exclude netai packages
    && sed -i 's/^\( *\)\(.*\# *exclude( *alpine *)\)$/\1# \2/g' ./Cargo.toml \
    # Include target-dependent packages
    && sed -i 's/^\( *\)\(.*\# *include( *[_0-9a-z-]\+ *)\)$/\1# \2/g' ./Cargo.toml \
    && sed -i "s/^\( *\)\# *\(.*\# *include( *$(uname -m) *)\)$/\1\2/g" ./Cargo.toml \
    # Replace minio-wasm package into minio
    && sed -i 's/rev *\= *\"[0-9a-f]\+\"\,//g' ./Cargo.toml \
    # Replace reqwest-wasm package into reqwest
    && sed -i 's/git *\= *\"[a-z\.\:\/\-]\+\"\, *package *\= *\"reqwest\(\-[a-z]\+\)\?\-wasm\", *//g' ./Cargo.toml \
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
