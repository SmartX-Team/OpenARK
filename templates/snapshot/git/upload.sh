#!/bin/bash

# Prehibit errors
set -e

# Download repository
git clone "${GIT_REPOSITORY}" ./snapshot
if [ ! -d ./snapshot ]; then
    # Create a new repository
    mkdir ./snapshot
    pushd ./snapshot

    # Init
    git init
    git remote add origin "${GIT_REPOSITORY}"
else
    pushd ./snapshot
fi

# Dump k8s snapshot
# TODO: to be implemented

# Commit & Push snapshot
# TODO: to be implemented

# Cleanup
popd
rm -rf ./snapshot
