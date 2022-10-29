#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Install Kiss Cluster                                  #
###########################################################

echo "- Installing kiss cluster ... "

# namespace & common
kubectl apply \
    -f "namespace.yaml"

# services
kubectl apply \
    -f "dnsmasq.yaml" \
    -f "docker-registry.yaml" \
    -f "http-proxy.yaml" \
    -f "matchbox.yaml" \
    -f "ntpd.yaml"

# ansible tasks
kubectl apply -f ./tasks/common.yaml
for dir in ./tasks/*; do
    # playbook directory
    if [ -d "$dir" ]; then
        kubectl create configmap "ansible-task-$(basename $dir)" \
            --namespace=kiss \
            --from-file=$dir \
            --output=yaml \
            --dry-run=client |
            kubectl apply -f -
    fi
done

# power configuration
kubectl apply -R -f "./power/*.yaml"

# kiss service
kubectl apply -R -f "./kiss-*.yaml"

# snapshot configuration
kubectl apply -R -f "./snapshot-*.yaml"

# force rolling-update kiss services
# note: https://github.com/kubernetes/kubernetes/issues/27081#issuecomment-327321981
for resource in "daemonsets" "deployments" "statefulsets"; do
    for object in $(
        kubectl get "$resource" \
            --no-headers \
            --namespace "kiss" \
            --output custom-columns=":metadata.name" \
            --selector 'kissService=true'
    ); do
        kubectl patch \
            --namespace "kiss" \
            --type "merge" \
            "$resource" "$object" --patch \
            "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"updatedDate\":\"$(date +'%s')\"}}}}}"
    done
done

# Finished!
echo "Installed!"
