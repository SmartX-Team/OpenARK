# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Load environment variables
set dotenv-load

# Configure environment variables
export ALPINE_VERSION := env_var_or_default('ALPINE_VERSION', '3.18')
export DESKTOP_DIST := env_var_or_default('DESKTOP_DIST', 'ubuntu')
export DESKTOP_VERSION := env_var_or_default('DESKTOP_VERSION', 'latest')
export OCI_BUILD_LOG_DIR := env_var_or_default('OCI_BUILD_LOG_DIR', './logs/')
export OCI_IMAGE := env_var_or_default('OCI_IMAGE', 'quay.io/ulagbulag/openark')
export OCI_IMAGE_VERSION := env_var_or_default('OCI_IMAGE_VERSION', 'latest')
export OCI_PLATFORMS := env_var_or_default('OCI_PLATFORMS', 'linux/arm64,linux/amd64')

export AWS_REGION := env_var_or_default('AWS_REGION', 'us-east-1')
export AWS_SECURE_TLS := if env_var("AWS_ENDPOINT_URL") =~ 'http://' { 'false' } else { 'true' }
export DEFAULT_RUNTIME_PACKAGE := env_var_or_default('DEFAULT_RUNTIME_PACKAGE', 'ark-cli')
export PIPE_MODEL := env_var_or_default('PIPE_MODEL', 'buildkit')

default:
  @just run

init-conda:
  conda install --yes \
    -c pytorch -c nvidia \
    autopep8 pip python \
    pytorch torchvision torchaudio pytorch-cuda=11.8
  pip install -r ./requirements.txt

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

oci-build *ARGS:
  mkdir -p "${OCI_BUILD_LOG_DIR}"
  docker buildx build \
    --file './Dockerfile' \
    --tag "${OCI_IMAGE}:${OCI_IMAGE_VERSION}" \
    --build-arg ALPINE_VERSION="${ALPINE_VERSION}" \
    --platform "${OCI_PLATFORMS}" \
    --pull \
    {{ ARGS }} \
    . 2>&1 | tee "${OCI_BUILD_LOG_DIR}/build-base-$( date -u +%s ).log"

oci-build-devel *ARGS:
  docker buildx build \
    --file './Dockerfile.devel' \
    --tag "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-devel" \
    --build-arg ALPINE_VERSION="${ALPINE_VERSION}" \
    --build-arg DESKTOP_DIST="${DESKTOP_DIST}" \
    --build-arg DESKTOP_VERSION="${DESKTOP_VERSION}" \
    --platform "linux/amd64" \
    --pull \
    {{ ARGS }} \
    . 2>&1 | tee "${OCI_BUILD_LOG_DIR}/build-devel-$( date -u +%s ).log"

oci-build-full *ARGS:
  docker buildx build \
    --file './Dockerfile.full' \
    --tag "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-full" \
    --build-arg ALPINE_VERSION="${ALPINE_VERSION}" \
    --platform "${OCI_PLATFORMS}" \
    --pull \
    {{ ARGS }} \
    . 2>&1 | tee "${OCI_BUILD_LOG_DIR}/build-full-$( date -u +%s ).log"

oci-push: (oci-build "--push")

oci-push-devel: (oci-build-devel "--push")

oci-push-full: (oci-build-full "--push")

oci-push-and-update-dash: oci-push
  kubectl -n dash rollout restart deploy gateway operator

oci-push-and-update-kiss: oci-push
  # kubectl -n kiss rollout restart deploy assets
  kubectl -n kiss rollout restart deploy dns gateway monitor operator

oci-push-and-update-kubegraph: oci-push
  kubectl -n kubegraph rollout restart deploy gateway kubegraph operator

oci-push-and-update-vine: oci-push
  kubectl -n dash rollout restart deploy bastion gateway operator
