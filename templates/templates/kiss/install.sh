#!/bin/bash
set -ex

kubectl apply \
    -f namespace.yaml

./external-dns.sh
./ingress-nginx.sh

kubectl apply -f dnsmasq.yaml
kubectl apply -f ingress-docker-registry.yaml
kubectl apply -f matchbox.yaml
