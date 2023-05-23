#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
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
NAMESPACE_DEFAULT="csi-rook-ceph"
ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED_DEFAULT="false"
ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT="false"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"
ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED="${ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED:-$ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED_DEFAULT}"
ROOK_CEPH_WAIT_UNTIL_DEPLOYED="${ROOK_CEPH_WAIT_UNTIL_DEPLOYED:-$ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT}"

###########################################################
#   Patch CephCluster                                  #
###########################################################

echo "- Patching CephCluster ... "

kubectl --namespace "${NAMESPACE}" patch cephcluster "${NAMESPACE}" \
    --type merge \
    -p '{"spec":{"cleanupPolicy":{"confirmation":"yes-really-destroy-data"}}}'

###########################################################
#   Remove Rook Ceph Cluster                              #
###########################################################

echo "- Removing Rook Ceph Cluster ... "

helm uninstall --namespace "${NAMESPACE}" "rook-ceph-cluster"

echo "- Waiting for removing Rook Ceph Cluster ... "
sleep 60

###########################################################
#   Remove Rook Ceph                                      #
###########################################################

echo "- Removing Rook Ceph ... "

helm uninstall --namespace "${NAMESPACE}" "rook-ceph"

echo "- Waiting for removing Rook Ceph ... "
sleep 60

###########################################################
#   Checking if Operator is already installed             #
###########################################################

echo "- Removing Namespace ... "

kubectl delete namespace "${NAMESPACE}"

# Finished!
echo "Installed!"
