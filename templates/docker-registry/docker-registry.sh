#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

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
