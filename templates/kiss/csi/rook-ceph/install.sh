#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
ROOK_CEPH_CHART_DEFAULT="https://charts.rook.io/release"
ROOK_CEPH_VERSION_DEFAULT="master"

# Set environment variables
ROOK_CEPH_CHART="${ROOK_CEPH_CHART:-$ROOK_CEPH_CHART_DEFAULT}"
ROOK_CEPH_VERSION="${ROOK_CEPH_VERSION:-$ROOK_CEPH_VERSION_DEFAULT}"

# Set derived environment variables
ROOK_CEPH_REPO="https://raw.githubusercontent.com/rook/rook/${ROOK_CEPH_VERSION}"
ROOK_CEPH_EXAMPLE="${ROOK_CEPH_REPO}/deploy/examples"

# Note: Ordered List
ROOK_CEPH_FILES=(
    "toolbox.yaml"
    "cluster-test.yaml"
    "csi/rbd/storageclass-test.yaml"
)

###########################################################
#   Install Rook-Ceph                                     #
###########################################################

echo "- Installing rook-ceph ..."

helm repo add rook-release "$ROOK_CEPH_CHART"
helm upgrade --install "rook-ceph" \
    "rook-release/rook-ceph" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values.yaml"

echo "- Waiting for deploying rook-ceph ..."
sleep 30

###########################################################
#   Configure Rook-Ceph                                   #
###########################################################

echo "- Configuring rook-ceph ..."

for file in $ROOK_CEPH_FILES; do
    kubectl apply --filename "$ROOK_CEPH_EXAMPLE/$file"
done

# Finished!
echo "Installed CSI!"
