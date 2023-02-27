#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Install Templates                                     #
###########################################################

echo "- Installing templates ... "

kubectl apply -f "./common.yaml"
for dir in ./*; do
    # playbook directory
    if [ -d "${dir}" ]; then
        kubectl create configmap "ansible-task-$(basename ${dir})" \
            --namespace=kiss \
            --from-file=${dir} \
            --output=yaml \
            --dry-run=client |
            kubectl apply -f -
    fi
done

# Finished!
echo "Installed!"
