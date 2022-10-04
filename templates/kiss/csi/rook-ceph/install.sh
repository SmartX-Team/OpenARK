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
HELM_CHART_DEFAULT="https://charts.rook.io/release"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"

###########################################################
#   Install Cluster Role                                  #
###########################################################

echo "- Installing ClusterRoles ..."

kubectl apply -f "./cluster-roles.yaml"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ..."

helm repo add "csi" "$HELM_CHART"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing CSI Operator ..."

helm upgrade --install "rook-ceph" \
    "csi/rook-ceph" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values-operator.yaml"

echo "- Waiting for deploying CSI Operator ..."
sleep 30

###########################################################
#   Install Storage Class                                 #
###########################################################

echo "- Installing Storage Class ..."

helm upgrade --install "rook-ceph-cluster" \
    "csi/rook-ceph-cluster" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values-cluster.yaml"

echo "- Waiting for deploying Storage Class ..."
sleep 30

# Finished!
echo "Installed CSI!"
