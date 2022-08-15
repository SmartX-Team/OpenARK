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
kubectl apply -R -f tasks

# kiss service
kubectl apply \
    -f tmp/assets.yaml \
    -f tmp/gateway.yaml
