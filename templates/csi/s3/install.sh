#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
HELM_IMAGE_DEFAULT="oci://cr.yandex/yc-marketplace/yandex-cloud/csi-s3/csi-s3"
NAMESPACE_DEFAULT="csi-s3"

# Set environment variables
HELM_IMAGE="${HELM_IMAGE:-$HELM_IMAGE_DEFAULT}"
NAMESPACE="${NAMESPACE:-$NAMESPACE_DEFAULT}"

# Parse from kiss-config
STORAGE_CLASS_NAME="$(
    kubectl -n kiss get secret kiss-config -o yaml |
        yq -r '.data.object_storage_class_name' |
        base64 --decode
)"
STORAGE_ENDPOINT="$(
    kubectl -n kiss get secret kiss-config -o yaml |
        yq -r '.data.object_storage_endpoint' |
        base64 --decode
)"
STORAGE_KEY_ACCESS="$(
    kubectl -n kiss get secret kiss-config -o yaml |
        yq -r '.data.object_storage_key_access' |
        base64 --decode
)"
STORAGE_KEY_SECRET="$(
    kubectl -n kiss get secret kiss-config -o yaml |
        yq -r '.data.object_storage_key_secret' |
        base64 --decode
)"

###########################################################
#   Check Environment Variables                           #
###########################################################

if [ "x${STORAGE_CLASS_NAME}" == "x" ]; then
    echo 'Skipping installation: "STORAGE_CLASS_NAME" not set'
    exit 0
fi

if [ "x${STORAGE_ENDPOINT}" == "x" ]; then
    echo 'Skipping installation: "STORAGE_ENDPOINT" not set'
    exit 0
fi

if [ "x${STORAGE_KEY_ACCESS}" == "x" ]; then
    echo 'Skipping installation: "STORAGE_KEY_ACCESS" not set'
    exit 0
fi

if [ "x${STORAGE_KEY_SECRET}" == "x" ]; then
    echo 'Skipping installation: "STORAGE_KEY_SECRET" not set'
    exit 0
fi

###########################################################
#   Download Helm Image                                   #
###########################################################

echo "- Downloading Helm image ... "

rm -rf "./csi-s3"
helm pull "${HELM_IMAGE}" --untar

###########################################################
#   Install Operator                                      #
###########################################################

echo "- Installing Operator ... "

helm upgrade --install "csi-s3" \
    "./csi-s3" \
    --create-namespace \
    --namespace "${NAMESPACE}-${STORAGE_CLASS_NAME}" \
    --set secret.endpoint="${STORAGE_ENDPOINT}" \
    --set secret.accessKey="${STORAGE_KEY_ACCESS}" \
    --set secret.secretKey="${STORAGE_KEY_SECRET}" \
    --set storageClass.name="${STORAGE_CLASS_NAME}" \
    --values "./values-operator.yaml"

# Finished!
echo "Installed!"
