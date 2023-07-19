#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Configuration                                         #
###########################################################

# Configure default environment variables
COMMAND_DEFAULT="${@:1}"
CSV_PATH_DEFAULT="./users.csv"

# Configure environment variables
COMMAND="${COMMAND:-$COMMAND_DEFAULT}"
CSV_PATH="${CSV_PATH:-$CSV_PATH_DEFAULT}"

TIMESTAMP="$(date -u +%s)"
TEMPLATE="
---
apiVersion: batch/v1
kind: Job
metadata:
  name: \"batch-script-${TIMESTAMP}\"
  namespace: \"__NAMESPACE__\"
spec:
  backoffLimit: 1
  suspend: false
  ttlSecondsAfterFinished: 1
  template:
    metadata:
      labels:
        name: \"batch-script\"
        timestamp: \"${TIMESTAMP}\"
    spec:
      affinity:
        nodeAffinity:
          # KISS ephemeral control plane nodes should be excluded
          requiredDuringSchedulingIgnoredDuringExecution:
            nodeSelectorTerms:
              - matchExpressions:
                  - key: node-role.kubernetes.io/kiss
                    operator: In
                    values:
                      - Compute
      containers:
        - name: script
          image: quay.io/ulagbulag/openark-vine-desktop:latest-rockylinux
          imagePullPolicy: Always
          command:
            - bash
            - -c
          args:
            - \"sudo chown 2000:2000 /run/user/2000 && ${COMMAND}\"
          env:
            - name: XDG_RUNTIME_DIR
              value: \"/run/user/2000\"
          securityContext:
            capabilities:
              add:
                - apparmor:unconfined
            privileged: true
          workingDir: /home/user
          volumeMounts:
            - name: home
              mountPath: /home/user
            - name: home-public
              mountPath: /mnt/public
            - name: home-static
              mountPath: /mnt/static
              readOnly: true
            - name: runtime-dbus
              mountPath: /run/dbus
            - name: runtime-user
              mountPath: \"/run/user/2000\"
            - name: tmp
              mountPath: /tmp
      hostIPC: true
      restartPolicy: Never
      securityContext:
        runAsNonRoot: false
        runAsUser: 2000
      serviceAccount: account
      terminationGracePeriodSeconds: 5
      volumes:
        - name: home
          persistentVolumeClaim:
            claimName: desktop
        - name: home-public
          persistentVolumeClaim:
            claimName: desktop-public
        - name: home-static
          persistentVolumeClaim:
            claimName: desktop-static
        - name: runtime-dbus
          hostPath:
            path: /run/dbus
            type: Directory
        - name: runtime-user
          emptyDir: {}
        - name: tmp
          emptyDir: {}
"

###########################################################
#   Run the given script for all given nodes              #
###########################################################

echo 'Running for user sessions'
for line in $(cat "${CSV_PATH}" | tail '+2'); do
  user_id="$(echo "${line}" | cut '-d,' -f1)"
  user_name="$(echo "${line}" | cut '-d,' -f2)"
  user_namespace="vine-session-${user_id}"

  echo -n "* ${user_name} -> "
  kubectl annotate namespace "${user_namespace}" \
    --overwrite \
    'scheduler.alpha.kubernetes.io/node-selector=' >/dev/null ||
    continue
  kubectl delete job -n "${user_namespace}" \
    -l 'name=batch-script' >/dev/null 2>/dev/null ||
    true
  echo "${TEMPLATE}" |
    sed "s/__NAMESPACE__/${user_namespace}/g" |
    kubectl apply -f - ||
    continue
done

###########################################################
#   Finished!                                             #
###########################################################

echo "OK"
