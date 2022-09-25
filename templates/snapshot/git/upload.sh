#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e

# Download repository
git clone "${GIT_REPOSITORY}" ./snapshot
pushd ./snapshot

# Dump k8s snapshot
mkdir -p ./kiss
kubectl get box -o yaml >./kiss/boxes.yaml
kubectl get -n kiss configmap -o yaml >./kiss/configmap.yaml
kubectl get -n kiss secret -o yaml >./kiss/secret.yaml

# Add
git add --force ./kiss

# Commit
git commit --message "Automatic Upload of Snapshot ($(date --rfc-3339=seconds))"

# Push
git branch -M main
git push --force --set-upstream origin main

# Cleanup
popd
rm -rf ./snapshot
