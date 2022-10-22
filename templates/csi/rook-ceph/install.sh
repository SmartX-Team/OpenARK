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
ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED_DEFAULT="true"
ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT="true"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED="${ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED:-$ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED_DEFAULT}"
ROOK_CEPH_WAIT_UNTIL_DEPLOYED="${ROOK_CEPH_WAIT_UNTIL_DEPLOYED:-$ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT}"

###########################################################
#   Install Cluster Role                                  #
###########################################################

echo "- Installing ClusterRoles ... "

kubectl apply -f "./cluster-roles.yaml"

###########################################################
#   Configure Helm Channel                                #
###########################################################

echo "- Configuring Helm channel ... "

helm repo add "csi" "$HELM_CHART"

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Checking Ceph Cluster is already installed ... "
if
    kubectl get namespace --no-headers "rook-ceph" \
        >/dev/null 2>/dev/null
then
    IS_FIRST=0
else
    IS_FIRST=1
fi

echo "- Installing CSI Operator ... "

helm upgrade --install "rook-ceph" \
    "csi/rook-ceph" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values-operator.yaml"

echo "- Waiting for deploying CSI Operator ... "
sleep 30

###########################################################
#   Install Storage Class                                 #
###########################################################

echo "- Installing Storage Class ... "

# do not update the number of monitors when re-deploying
if [ "$IS_FIRST" -eq 0 ]; then
    ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED="false"
fi

# tweaks - use single monitor node until ceph cluster is deployed
# FIXME: Rook-Ceph on Flatcar OS is not working on mon > 1
# See also: https://github.com/rook/rook/issues/10110
if [ "$ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED" == "true" ]; then
    NUM_MONS=$(yq ".cephClusterSpec.mon.count" "./values-cluster.yaml")
    yq --inplace ".cephClusterSpec.mon.count = 1" "./values-cluster.yaml"
fi

helm upgrade --install "rook-ceph-cluster" \
    "csi/rook-ceph-cluster" \
    --create-namespace \
    --namespace "rook-ceph" \
    --values "./values-cluster.yaml"

###########################################################
#   Wait for deploying Storage Class                      #
###########################################################

if [ "$ROOK_CEPH_WAIT_UNTIL_DEPLOYED" == "true" ]; then
    echo -n "- Waiting for deploying Ceph Tools ... "
    kubectl --namespace "rook-ceph" rollout status deployment "rook-ceph-tools" >/dev/null
    echo "OK"

    echo -n "- Waiting for deploying Storage Classes ... "
    function wait_all_storage_class() {
        while :; do
            local COMPLETED=1
            for storageclass in "blockpool" "filesystem" "objectstore"; do
                local PHASE=$(
                    kubectl --namespace "rook-ceph" get "ceph$storageclass" "ceph-$storageclass" \
                        --output jsonpath --template '{.status.phase}' \
                        2>/dev/null
                )
                case "$PHASE" in
                "Connected" | "Ready")
                    continue
                    ;;
                *)
                    local COMPLETED=0
                    break
                    ;;
                esac
            done

            if [ "$COMPLETED" -eq 1 ]; then
                break
            fi

            # pass some times
            sleep 5
        done
    }
    wait_all_storage_class
    echo "OK"

    echo -n "- Waiting for deploying Ceph Cluster ... "
    function wait_ceph_cluster() {
        while :; do
            local PHASE=$(
                kubectl --namespace "rook-ceph" get "cephcluster" "rook-ceph" \
                    --output jsonpath --template '{.status.phase}' \
                    2>/dev/null
            )
            case "$PHASE" in
            "Ready")
                continue
                ;;
            *)
                break
                ;;
            esac

            # pass some times
            sleep 5
        done
    }
    wait_ceph_cluster
    echo "OK"

    # tweaks - use single monitor nodes until ceph cluster is deployed
    # FIXME: Rook-Ceph on Flatcar OS is not working on mon > 1
    # See also: https://github.com/rook/rook/issues/10110
    if [ "$ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED" == "true" ]; then
        if [ "$NUM_MONS" != "1" ]; then
            yq --inplace ".cephClusterSpec.mon.count = $NUM_MONS" "./values-cluster.yaml"

            helm upgrade --install "rook-ceph-cluster" \
                "csi/rook-ceph-cluster" \
                --create-namespace \
                --namespace "rook-ceph" \
                --values "./values-cluster.yaml"
            wait_ceph_cluster
        fi
    fi
fi

# Finished!
echo "Installed CSI!"
