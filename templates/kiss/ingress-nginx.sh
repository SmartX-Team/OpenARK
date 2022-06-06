#!/bin/bash

helm upgrade --install \
    ingress-nginx ingress-nginx \
    --repo https://kubernetes.github.io/ingress-nginx \
    --namespace kiss \
    --values ingress-nginx-values.yaml
