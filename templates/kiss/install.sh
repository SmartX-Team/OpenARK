#!/bin/bash
set -ex

# namespace & common
kubectl apply \
    -f namespace.yaml

# services
kubectl apply \
    -f dnsmasq.yaml \
    -f docker-registry.yaml \
    -f http-proxy.yaml \
    -f matchbox.yaml \
    -f ntpd.yaml \
    -f snapshot.yaml

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

# force rolling-update kiss services
# note: https://github.com/kubernetes/kubernetes/issues/27081#issuecomment-327321981
kubectl patch -R -f "./kiss-*.yaml" -p \
    "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"updatedDate\":\"$(date +'%s')\"}}}}}"
