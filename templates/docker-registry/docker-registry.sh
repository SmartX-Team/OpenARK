#!/bin/bash

helm upgrade --install \
    docker-registry docker-registry \
    --repo https://helm.twun.io \
    --namespace kube-system \
    --create-namespace \
    --values values.yaml

kubectl create secret docker-registry private \
    --namespace kube-system \
    --docker-server=docker-registry.kube-system \
    --docker-username=user \
    --docker-password=user
