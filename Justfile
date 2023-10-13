# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Load environment variables
set dotenv-load

# Configure environment variables
export ALPINE_VERSION := env_var_or_default('ALPINE_VERSION', '3.17')
export OCI_IMAGE := env_var_or_default('OCI_IMAGE', 'quay.io/ulagbulag/openark')
export OCI_IMAGE_VERSION := env_var_or_default('OCI_IMAGE_VERSION', 'latest')
export OCI_PLATFORMS := env_var_or_default('OCI_PLATFORMS', 'linux/arm64,linux/amd64')

export AWS_UNSAFE_ALLOW_HTTP := if "${AWS_ENDPOINT_URL}" == 'http' { 'true' } else { 'false' }
export AWS_REGION := env_var_or_default('AWS_REGION', 'us-east-1')
export DEFAULT_RUNTIME_PACKAGE := env_var_or_default('DEFAULT_RUNTIME_PACKAGE', 'dash-cli')
export PIPE_MODEL := env_var_or_default('PIPE_MODEL', 'buildkit')

DOCKER_BUILDKIT_CACHE := "type=s3,bucket=${PIPE_MODEL},region=${AWS_REGION},secure=${AWS_UNSAFE_ALLOW_HTTP}"

default:
  @just run

init-conda:
  conda create -n dash \
    -c pytorch -c nvidia \
    autopep8 pip python \
    pytorch torchvision torchaudio pytorch-cuda=11.8

fmt:
  cargo fmt --all

build: fmt
  cargo build --all --workspace

clippy: fmt
  cargo clippy --all --workspace

test: clippy
  cargo test --all --workspace

run *ARGS:
  cargo run --package "${DEFAULT_RUNTIME_PACKAGE}" --release -- {{ ARGS }}

oci-build:
  docker buildx build \
    --file './Dockerfile' \
    --tag "${OCI_IMAGE}:${OCI_IMAGE_VERSION}" \
    --build-arg ALPINE_VERSION="${ALPINE_VERSION}" \
    --cache-from "{{ DOCKER_BUILDKIT_CACHE }}" \
    --cache-to "{{ DOCKER_BUILDKIT_CACHE }}" \
    --platform "${OCI_PLATFORMS}" \
    --pull \
    --push \
    .

oci-build-devel:
  docker build \
    --file './Dockerfile.devel' \
    --tag "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-devel" \
    --build-arg ALPINE_VERSION="${ALPINE_VERSION}" \
    .

oci-build-full:
  docker buildx build \
    --file './Dockerfile.full' \
    --tag "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-full" \
    --build-arg ALPINE_VERSION="${ALPINE_VERSION}" \
    --cache-from "{{ DOCKER_BUILDKIT_CACHE }}" \
    --cache-to "{{ DOCKER_BUILDKIT_CACHE }}" \
    --platform "${OCI_PLATFORMS}" \
    --pull \
    --push \
    .

oci-push: oci-build

oci-push-devel: oci-build-devel
  docker push "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-devel"

oci-push-full: oci-build-full

oci-push-and-update-dash: oci-push
  kubectl -n dash delete pods --selector name=controller
  kubectl -n dash delete pods --selector name=gateway

oci-push-and-update-kiss: oci-push
  kubectl -n dash delete pods --selector name=gateway
  kubectl -n kiss delete pods --selector name=controller
  kubectl -n kiss delete pods --selector name=gateway
  kubectl -n kiss delete pods --selector name=monitor

oci-push-and-update-vine: oci-push
  kubectl -n dash delete pods --selector name=gateway
  kubectl -n vine delete pods --selector name=bastion
  kubectl -n vine delete pods --selector name=controller
