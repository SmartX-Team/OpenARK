#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e

# Check repository
if [ "${GIT_REPOSITORY}" == "" ]; then
    echo "Skipping snapshot job: Git repository is not set"
    exit 0
fi

# Configure git
git config --global user.email "${GIT_USER_EMAIL}"
git config --global user.name "${GIT_USER_NAME}"

# Download repository
git clone "${GIT_REPOSITORY}" "./snapshot"
cd "./snapshot"

# Checkout branch
# FIXME: is it working?
git checkout -B "${GIT_BRANCH}"

# Dump k8s snapshot
mkdir -p "./kiss"
kubectl get box -o yaml >"./kiss/boxes.yaml"
kubectl get -n kiss configmap -o yaml >"./kiss/configmap.yaml"
kubectl get -n kiss secret -o yaml >"./kiss/secret.yaml"

# Add
git add --force "./kiss"

# Commit
git commit --message "Automatic Upload of Snapshot ($(date -u +'%Y-%m-%dT%H:%M:%SZ'))" || true

# Push
git push --force --set-upstream "origin" "${GIT_BRANCH}"

# Cleanup
cd -
rm -rf "./snapshot"
