#!/bin/bash

helm upgrade --install \
    kiss-exdns-1 k8s-gateway \
    --repo https://ori-edge.github.io/k8s_gateway \
    --namespace kiss \
    --values external-dns-exdns-1.yaml

kubectl apply -f dns_proxy.yaml
