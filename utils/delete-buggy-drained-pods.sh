#!/bin/bash

kubectl get pods --all-namespaces -o json |
    jq -r '.items[] | select(.status.phase == "Failed" or .status.reason == "NodeLost") | .metadata.name + " " + .metadata.namespace' |
    while read pod namespace; do
        kubectl delete pod "${pod}" --namespace "${namespace}"
    done
