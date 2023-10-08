# Configure environment variables
export ALPINE_VERSION := env_var_or_default('ALPINE_VERSION', '3.17')
export OCI_IMAGE := env_var_or_default('OCI_IMAGE', 'quay.io/ulagbulag/openark')
export OCI_IMAGE_VERSION := env_var_or_default('OCI_IMAGE_VERSION', 'latest')
export OCI_PLATFORMS := env_var_or_default('OCI_PLATFORMS', 'linux/arm64,linux/amd64')

export DEFAULT_RUNTIME_PACKAGE := env_var_or_default('DEFAULT_RUNTIME_PACKAGE', 'dash-cli')

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
    --platform "${OCI_PLATFORMS}" \
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
    --platform "${OCI_PLATFORMS}" \
    .

oci-push: oci-build
  docker push "${OCI_IMAGE}:${OCI_IMAGE_VERSION}"

oci-push-devel: oci-build-devel
  docker push "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-devel"

oci-push-full: oci-build-full
  docker push "${OCI_IMAGE}:${OCI_IMAGE_VERSION}-full"

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
