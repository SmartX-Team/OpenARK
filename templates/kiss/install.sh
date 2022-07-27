#!/bin/bash
set -ex

# namespace & common
kubectl apply \
    -f namespace.yaml

# dependencies
./external-dns.sh
./ingress-nginx.sh
sleep 30

# services
kubectl apply \
    -f dnsmasq.yaml \
    -f ingress-docker-registry.yaml \
    -f matchbox.yaml

# ansible tasks
kubectl apply -R -f tasks

# kiss proxy
kubectl apply \
    -f tmp/assets.yaml \
    -f tmp/gateway.yaml \
    -f http_proxy.yaml \
    -f ntpd.yaml
