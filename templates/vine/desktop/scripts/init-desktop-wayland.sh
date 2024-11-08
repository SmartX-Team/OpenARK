#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Configure environment variables
export DISPLAY="${DISPLAY:-:0}"
echo "export DISPLAY=\"${DISPLAY}\"" >>"${__ENV_HOME}"

# Create an empty X11 socket directory, if not exists
mkdir -p "/tmp/.X11-unix"

# Detect GPU Devices
if nvidia-smi >/dev/null 2>/dev/null; then
    export VK_DRIVER_FILES="/usr/share/vulkan/icd.d/nvidia_icd.json"
    export VK_ICD_FILENAMES="${VK_DRIVER_FILES}"

    # Make these environment variables persistent
    echo "export VK_DRIVER_FILES=\"${VK_DRIVER_FILES}\"" >>"${__ENV_HOME}"
    echo "export VK_ICD_FILENAMES=\"${VK_ICD_FILENAMES}\"" >>"${__ENV_HOME}"
fi

# Create a xwayland session, if not exists
if [ ! -S "/tmp/.X11-unix/X$(echo "${DISPLAY}" | grep -Po '[0-9]+$')" ]; then
    # Configure environment variables
    export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
    echo "export WAYLAND_DISPLAY=\"${WAYLAND_DISPLAY}\"" >>"${__ENV_HOME}"

    # Configure wayland
    WAYLAND_BACKEND_RDP='rdp-backend.so'
    WAYLAND_BACKEND_VNC='vnc-backend.so'

    WAYLAND_ARGS="${WAYLAND_ARGS} --socket=${WAYLAND_DISPLAY}"
    WAYLAND_ARGS="${WAYLAND_ARGS} --xwayland"
    WAYLAND_BACKEND="${WAYLAND_BACKEND:-$WAYLAND_BACKEND_VNC}"
    WAYLAND_TLS_HOME="${HOME}/.rdp"

    # Detect backend kind
    if [ "x${WAYLAND_BACKEND}" == "x${WAYLAND_BACKEND_RDP}" ]; then
        WAYLAND_BACKEND_KIND='rdp'
    elif [ "x${WAYLAND_BACKEND}" == "x${WAYLAND_BACKEND_VNC}" ]; then
        WAYLAND_BACKEND_KIND='vnc'
    else
        WAYLAND_BACKEND_KIND=''
    fi

    # Configure remote backend
    if [ "x${WAYLAND_BACKEND_KIND}" != 'x' ]; then
        # Generate a TLS key pair
        if [ ! -f "${WAYLAND_TLS_HOME}/${HOSTNAME}.crt" ] ||
            [ ! -f "${WAYLAND_TLS_HOME}/${HOSTNAME}.key" ]; then
            mkdir -p "${WAYLAND_TLS_HOME}"
            chmod 700 "${WAYLAND_TLS_HOME}"
            winpr-makecert -rdp -path "${WAYLAND_TLS_HOME}" >/dev/null
        fi

        # Register the TLS key pair
        WAYLAND_ARGS="${WAYLAND_ARGS} "--${WAYLAND_BACKEND_KIND}-tls-cert" ${WAYLAND_TLS_HOME}/${HOSTNAME}.crt"
        WAYLAND_ARGS="${WAYLAND_ARGS} "--${WAYLAND_BACKEND_KIND}-tls-key" ${WAYLAND_TLS_HOME}/${HOSTNAME}.key"
    fi

    # Detect GPU Devices
    if nvidia-smi >/dev/null 2>/dev/null; then
        export __GLX_VENDOR_LIBRARY_NAME="nvidia"
        export __NV_PRIME_RENDER_OFFLOAD="1"

        # Make these environment variables persistent
        echo "export __GLX_VENDOR_LIBRARY_NAME=\"${__GLX_VENDOR_LIBRARY_NAME}\"" >>"${__ENV_HOME}"
        echo "export __NV_PRIME_RENDER_OFFLOAD=\"${__NV_PRIME_RENDER_OFFLOAD}\"" >>"${__ENV_HOME}"
    fi

    # Disable default weston terminal
    if sudo whoami >/dev/null; then
        sudo ln -sf /usr/bin/xfce4-terminal /usr/bin/weston-terminal
    fi

    weston --backend="${WAYLAND_BACKEND}" ${WAYLAND_ARGS} &
fi
