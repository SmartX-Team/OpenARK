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
ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT="false"

# Set environment variables
HELM_CHART="${HELM_CHART:-$HELM_CHART_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"
ROOK_CEPH_WAIT_UNTIL_DEPLOYED="${ROOK_CEPH_WAIT_UNTIL_DEPLOYED:-$ROOK_CEPH_WAIT_UNTIL_DEPLOYED_DEFAULT}"

# Provision storage nodes
NUM_STORAGE_NODES="$(
    kubectl get nodes -l node-role.kubernetes.io/kiss=Storage -o name --no-headers |
        wc -l
)"

EXTRA_VALUES=""

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
until kubectl get crd 'cephclusters.ceph.rook.io'; do
    sleep 1
done

###########################################################
#   Install Ceph Cluster                                  #
###########################################################

echo "- Installing Ceph Cluster ... "

# Resize the number of monitors
NUM_MONS=$(yq ".cephClusterSpec.mon.count" "./values-cluster.yaml")
if [ "x$((${NUM_STORAGE_NODES} < ${NUM_MONS}))" == "x1" ]; then
    EXTRA_VALUES="${EXTRA_VALUES} --set cephClusterSpec.mon.count=1"
fi

# Resize the number of managers
NUM_MGRS=$(yq ".cephClusterSpec.mgr.count" "./values-cluster.yaml")
if [ "x$((${NUM_STORAGE_NODES} < ${NUM_MGRS}))" == "x1" ]; then
    EXTRA_VALUES="${EXTRA_VALUES} --set cephClusterSpec.mgr.count=1"
fi

helm upgrade --install "rook-ceph-cluster" \
    "${NAMESPACE}/rook-ceph-cluster" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set "operatorNamespace=${NAMESPACE}" \
    --values "./values-cluster.yaml" \
    ${EXTRA_VALUES}

echo -n "- Waiting for deploying Ceph Tools ... "
kubectl --namespace "${NAMESPACE}" rollout status deployment "rook-ceph-tools"
echo "OK"

echo -n "- Waiting for deploying Ceph Cluster ... "
function wait_ceph_cluster() {
    while :; do
        local PHASE=$(
            kubectl --namespace "${NAMESPACE}" get "cephcluster" "csi-rook-ceph" \
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
