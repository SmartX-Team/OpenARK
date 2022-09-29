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

# Set environment variables
ROOK_CEPH_CHART="${ROOK_CEPH_CHART:-$ROOK_CEPH_CHART_DEFAULT}"

###########################################################
#   Install Cluster Role                                  #
###########################################################

echo "- Installing ClusterRoles ..."

kubectl apply -f "./cluster-roles.yaml"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring rook-ceph helm channel ..."

helm repo add "rook-release" "$ROOK_CEPH_CHART"

###########################################################
#   Install Rook-Ceph Operator                            #
###########################################################

echo "- Installing rook-ceph operator ..."

helm upgrade --install "rook-ceph" \
    "rook-release/rook-ceph" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values-operator.yaml"

echo "- Waiting for deploying rook-ceph operator ..."
sleep 30

###########################################################
#   Install Rook-Ceph Cluster                             #
###########################################################

echo "- Installing rook-ceph cluster ..."

helm upgrade --install "rook-ceph-cluster" \
    "rook-release/rook-ceph-cluster" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values-cluster.yaml"

echo "- Waiting for deploying rook-ceph cluster ..."
sleep 30

# Finished!
echo "Installed CSI!"
