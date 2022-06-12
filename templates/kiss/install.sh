#!/bin/bash
set -ex

# namespace & common
kubectl apply \
    -f namespace.yaml

# dependencies
./external-dns.sh
./ingress-nginx.sh

# services
kubectl apply -f dnsmasq.yaml
kubectl apply -f ingress-docker-registry.yaml
kubectl apply -f matchbox.yaml

# ansible tasks
kubectl apply \
    -f commission.yaml \
    -f common.yaml

# TODO: kiss proxy
kubectl apply \
    -f tmp/assets.yaml \
    -f tmp/gateway.yaml
