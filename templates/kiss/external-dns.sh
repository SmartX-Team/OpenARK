#!/bin/bash

helm upgrade --install \
    kiss-exdns-1 k8s-gateway \
    --repo https://ori-edge.github.io/k8s_gateway \
    --namespace kiss \
    --values external-dns-exdns-1.yaml
helm upgrade --install \
    kiss-exdns-2 k8s-gateway \
    --repo https://ori-edge.github.io/k8s_gateway \
    --namespace kiss \
    --values external-dns-exdns-2.yaml
