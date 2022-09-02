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
    -f ntpd.yaml

# ansible tasks
kubectl apply -R -f "./tasks"

# kiss service
kubectl apply -R -f "./kiss-*.yaml"

# force rolling-update kiss services
# note: https://github.com/kubernetes/kubernetes/issues/27081#issuecomment-327321981
kubectl patch -R -f "./kiss-*.yaml" -p \
    "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"updatedDate\":\"`date +'%s'`\"}}}}}"
