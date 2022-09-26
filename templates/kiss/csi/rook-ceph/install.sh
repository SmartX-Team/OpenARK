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

for file in ${ROOK_CEPH_FILES[@]}; do
    # download file
    file_local="/tmp/$(echo $file | sha256sum | awk '{print $1}').yaml"
    wget -O "$file_local" "$ROOK_CEPH_EXAMPLE/$file"

    # apply some tweaks
    ## CephCluster
    if [ "$file" == "cluster-test.yaml" ]; then
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.daemonHealth.mon.disabled = true' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.daemonHealth.osd.disabled = false' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.daemonHealth.osd.interval = "60s"' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.daemonHealth.status.disabled = false' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.daemonHealth.status.interval = "60s"' "$file_local"
        # Change pod liveness probe timing or threshold values. Works for all mon,mgr,osd daemons.
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.livenessProbe.mon.disabled = true' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.livenessProbe.mgr.disabled = true' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.livenessProbe.osd.disabled = false' "$file_local"
        # Change pod startup probe timing or threshold values. Works for all mon,mgr,osd daemons.
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.startupProbe.mon.disabled = true' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.startupProbe.mgr.disabled = true' "$file_local"
        yq --inplace 'select(.kind=="CephCluster").spec.healthCheck.startupProbe.osd.disabled = false' "$file_local"
    fi

    # apply to the cluster
    kubectl apply --filename "$file_local"

    # remove the downloaded file
    rm -f "$file_local" || true
done

# Finished!
echo "Installed CSI!"
