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
#   Install Storage Class                                 #
###########################################################

echo "- Installing Storage Class ... "

# tweaks - use single monitor node until ceph cluster is deployed
# FIXME: Rook-Ceph on Flatcar OS is not working on mon > 1
# See also: https://github.com/rook/rook/issues/10110
NUM_MONS=$(yq ".cephClusterSpec.mon.count" "./values-cluster.yaml")
TEMPLATE_PATH="./tmp-values-cluster.yaml"
if [ "x$((${NUM_STORAGE_NODES} < ${NUM_MONS}))" == "x1" ]; then
    yq ".cephClusterSpec.mon.count = 1" "./values-cluster.yaml" >"${TEMPLATE_PATH}"
else
    cp "./values-cluster.yaml" "${TEMPLATE_PATH}"
fi

helm upgrade --install "rook-ceph-cluster" \
    "${NAMESPACE}/rook-ceph-cluster" \
    --create-namespace \
    --namespace "${NAMESPACE}" \
    --set "operatorNamespace=${NAMESPACE}" \
    --values "${TEMPLATE_PATH}"
rm -f "${TEMPLATE_PATH}"

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
