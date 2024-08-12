#!/bin/bash
# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e
# Verbose
set -x

###########################################################
#   Install KISS Cluster                                  #
###########################################################

# Define a KISS cluster installer function
function install_kiss_cluster() {
    local cluster_name="$(
        kubectl get configmap "kiss-config" \
            --namespace kiss \
            --output jsonpath \
            --template "{.data.kiss_cluster_name}"
    )"

    echo "- Installing KISS cluster ... "

    # namespace & common
    kubectl apply \
        -f "namespace.yaml"

    if [ "x${cluster_name}" == "xdefault" ]; then
        # services
        kubectl apply \
            -f "dnsmasq.yaml" \
            -f "docker-registry.yaml" \
            -f "assets.yaml" \
            -f "ntpd.yaml"

        # ansible tasks
        pushd "tasks"
        ./install.sh
        popd

        # assets
        pushd "assets"
        ./install.sh
        popd

        # kiss service
        kubectl apply -R -f "./kiss-*.yaml"
    else
        # kiss manager
        kubectl apply -f "./kiss-manager.yaml"
    fi

    # snapshot configuration
    kubectl apply -R -f "./snapshot-*.yaml"

    # force rolling-update kiss services
    # note: https://github.com/kubernetes/kubernetes/issues/27081#issuecomment-327321981
    for resource in "daemonsets" "deployments" "statefulsets"; do
        for object in $(
            kubectl get "${resource}" \
                --no-headers \
                --namespace "kiss" \
                --output custom-columns=":metadata.name" \
                --selector 'kissService=true'
        ); do
            kubectl patch \
                --namespace "kiss" \
                --type "merge" \
                "${resource}" "${object}" --patch \
                "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"updatedDate\":\"$(date +'%s')\"}}}}}"
        done
    done

    # Finished!
    echo "OK"
}

###########################################################
#   Define Service Operator Installer                     #
###########################################################

# Define a service installer template function
function __install_service() {
    local kind="$1"
    local image="$2"
    local name="$(echo "$3" | sed 's/\_/\-/g')"
    local is_enabled="$4"

    local image="$(echo "${image_template}" | sed "s/__NAME__/${name}/g")"

    # Check if flag is enabled
    local NEED_INSTALL=1
    if [ $(echo "${is_enabled}" | awk '{print tolower($0)}') != "true" ]; then
        echo -n "- Skipping installing ${name}/${kind} ... "
        local NEED_INSTALL=0
    fi

    if [ "x${NEED_INSTALL}" == "x1" ]; then
        # Install service
        echo -n "- Installing ${name}/${kind} in background ... "
        cat "./templates/service-installer.yaml" |
            sed "s/__KIND__/${kind}/g" |
            sed "s/__NAME__/${name}/g" |
            sed "s/__IMAGE__/$(
                echo "${image}" |
                    sed 's/[^a-zA-Z0-9]/\\&/g; 1{$s/^$/""/}; 1!s/^/"/; $!s/$/"/'
            )/g" |
            kubectl apply -f -
    fi

    # Finished!
    echo "OK"
}

# Define service operators installer function
function __install_service_all() {
    local kind="$1"

    local image_template="$(
        kubectl get configmap "kiss-config" \
            --namespace kiss \
            --output jsonpath \
            --template "{.data.service_${kind}_installer_image_template}"
    )"

    # Install multi-vendor services
    local key_prefix_is_enabled="service_${kind}_enable_"
    local key_re_is_enabled="^${key_prefix_is_enabled}\([0-9a-z_]\+\).*\$"

    for name in $(
        kubectl get configmap "kiss-config" \
            --namespace kiss \
            --output yaml |
            yq -r '.data | keys | @tsv' |
            tr '\t' '\n' |
            grep "${key_re_is_enabled}" |
            sed "s/${key_re_is_enabled}/\1/g"
    ); do
        local is_enabled="$(
            kubectl get configmap "kiss-config" \
                --namespace kiss \
                --output jsonpath \
                --template "{.data.${key_prefix_is_enabled}${name}}"
        )"
        __install_service "${kind}" "${image_template}" "${name}" "${is_enabled}"
    done
}

# Define all kind of service operators installer function
function install_services() {
    local key_prefix_kind="service_"

    # Install multi-vendor services
    local key_suffix_kind="_installer_image_template"
    local key_re_kind="^${key_prefix_kind}\([0-9a-z_]\+\)${key_suffix_kind}\$"

    for kind in $(
        kubectl get configmap "kiss-config" \
            --namespace kiss \
            --output yaml |
            yq -r '.data | keys | @tsv' |
            tr '\t' '\n' |
            grep "${key_re_kind}" |
            sed "s/${key_re_kind}/\1/g"
    ); do
        __install_service_all "${kind}"
    done

    # Install IPIS
    local ipis_image="$(
        kubectl get configmap "kiss-config" \
            --namespace kiss \
            --output jsonpath \
            --template "{.data.service_ipis_installer_image}"
    )"
    local ipis_is_enabled="$(
        kubectl get configmap "kiss-config" \
            --namespace kiss \
            --output jsonpath \
            --template "{.data.service_ipis_enable}"
    )"
    __install_service "ipis" "${ipis_image}" "ipis" "${ipis_is_enabled}"
}

# Define all kind of legacy service operators uninstaller function
function uninstall_services() {
    kubectl --namespace kiss delete deploy controller || true
}

###########################################################
#   Main Function                                         #
###########################################################

# Define a main function
function main() {
    # Install a KISS cluster within k8s cluster
    install_kiss_cluster

    # Install services operators
    install_services

    # Uninstall legacy services
    uninstall_services

    # Finished!
    echo "Installed!"
}

# Execute main function
main "$@" || exit 1
