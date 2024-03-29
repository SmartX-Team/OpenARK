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
NAMESPACE_DEFAULT="csi-rook-ceph"
ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED_DEFAULT="false"
ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT="false"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"
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

helm repo add "${NAMESPACE}" "${HELM_CHART}"

###########################################################
#   Checking if Operator is already installed             #
###########################################################

echo "- Checking Operator is already installed ... "
if
    kubectl get namespace --no-headers "${NAMESPACE}" \
        >/dev/null 2>/dev/null
then
    IS_FIRST=0
else
    IS_FIRST=1
fi

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "rook-ceph" \
    "${NAMESPACE}/rook-ceph" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --values "./values-operator.yaml"

echo "- Waiting for deploying Operator ... "
sleep 30

###########################################################
#   Install Storage Class                                 #
###########################################################

echo "- Installing Storage Class ... "

# do not update the number of monitors when re-deploying
if [ "x${IS_FIRST}" == "x0" ]; then
    ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED="false"
fi

# tweaks - use single monitor node until ceph cluster is deployed
# FIXME: Rook-Ceph on Flatcar OS is not working on mon > 1
# See also: https://github.com/rook/rook/issues/10110
if [ "x${ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED}" == "xtrue" ]; then
    NUM_MONS=$(yq ".cephClusterSpec.mon.count" "./values-cluster.yaml")
    yq --inplace ".cephClusterSpec.mon.count = 1" "./values-cluster.yaml"
fi

helm upgrade --install "rook-ceph-cluster" \
    "${NAMESPACE}/rook-ceph-cluster" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set "operatorNamespace=${NAMESPACE}" \
    --values "./values-cluster.yaml"

###########################################################
#   Wait for deploying Storage Class                      #
###########################################################

if [ "x${ROOK_CEPH_WAIT_UNTIL_DEPLOYED}" == "xtrue" ]; then
    echo -n "- Waiting for deploying Ceph Tools ... "
    kubectl --namespace "${NAMESPACE}" rollout status deployment "rook-ceph-tools" >/dev/null
    echo "OK"

    echo -n "- Waiting for deploying Storage Classes ... "
    function wait_all_storage_class() {
        while :; do
            local COMPLETED=1
            for storageclass in "blockpool" "filesystem" "objectstore"; do
                local PHASE=$(
                    kubectl --namespace "${NAMESPACE}" get "ceph${storageclass}" "ceph-${storageclass}" \
                        --output jsonpath --template '{.status.phase}' \
                        2>/dev/null
                )
                case "${PHASE}" in
                "Connected" | "Ready")
                    continue
                    ;;
                *)
                    local COMPLETED=0
                    break
                    ;;
                esac
            done

            if [ "x${COMPLETED}" == "x1" ]; then
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
                kubectl --namespace "${NAMESPACE}" get "cephcluster" "rook-ceph" \
                    --output jsonpath --template '{.status.phase}' \
                    2>/dev/null
            )
            case "${PHASE}" in
            "Ready")
                break
                ;;
            *)
                # pass some times
                sleep 5
                continue
                ;;
            esac
        done
    }
    wait_ceph_cluster
    echo "OK"

    # tweaks - use single monitor nodes until ceph cluster is deployed
    # FIXME: Rook-Ceph on Flatcar OS is not working on mon > 1
    # See also: https://github.com/rook/rook/issues/10110
    if [ "x${ROOK_CEPH_USE_SINGLE_MON_UNTIL_DEPLOYED}" == "xtrue" ]; then
        if [ "x${NUM_MONS}" != "x1" ]; then
            yq --inplace ".cephClusterSpec.mon.count = ${NUM_MONS}" "./values-cluster.yaml"

            helm upgrade --install "rook-ceph-cluster" \
                "${NAMESPACE}/rook-ceph-cluster" \
                --create-namespace \
                --namespace "${NAMESPACE}" \
                --set "operatorNamespace=${NAMESPACE}" \
                --values "./values-cluster.yaml"
            wait_ceph_cluster
        fi
    fi
fi

###########################################################
#   Patch Service Monitor                                 #
###########################################################

echo "- Patching Service Monitor ... "

until kubectl get servicemonitor 'rook-ceph-mgr' \
    --namespace "${NAMESPACE}" \
    >/dev/null 2>/dev/null; do
    sleep 3
done

kubectl get servicemonitor 'rook-ceph-mgr' \
    --namespace "${NAMESPACE}" \
    --output yaml |
    yq 'del(.spec.selector.matchLabels.mgr_role)' |
    yq ".spec.selector.matchLabels.rook_cluster=\"${NAMESPACE}\"" |
    kubectl replace -f -

# Finished!
echo "Installed!"
