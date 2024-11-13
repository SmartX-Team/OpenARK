#!/bin/bash

# TODO: auth & import from main cluster!

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Get OS Version
source /etc/os-release

# Pre-Hook
## Desktop Environment Configuration
_IS_DESKTOP="false"
_IS_NVIDIA_MANUAL="false"

# Advanced Network configuration
mkdir -p /etc/NetworkManager/system-connections/

## Disable Kernel Module: rndis_host
cat <<EOF >/etc/default/grub.d/10-blacklist-rndis_host.cfg
GRUB_CMDLINE_LINUX="modprobe.blacklist=rndis_host"
EOF
cat <<EOF >/etc/modprobe.d/blacklist-rndis_host.conf
blacklist rndis_host
EOF

## Disable Power Saving Mode (iwlmvm)
cat <<EOF >/etc/modprobe.d/iwlmvm.conf
options iwlmvm power_scheme=1
EOF

## Disable Power Saving Mode (iwlwifi)
cat <<EOF >/etc/modprobe.d/iwlwifi.conf
options iwlwifi power_save=0
EOF

## Disable Power Saving Mode on NetworkManager
mkdir -p /etc/NetworkManager/conf.d/
rm -f /etc/NetworkManager/conf.d/default-wifi-powersave-on.conf
cat <<EOF >/etc/NetworkManager/conf.d/default-wifi-powersave-off.conf
[connection]
wifi.powersave = 2
EOF

## Diasble systemd-networkd
systemctl disable \
    systemd-networkd.service \
    systemd-networkd.socket \
    systemd-networkd-wait-online.service

## Deactivate systemd-resolved
mkdir -p /etc/NetworkManager/conf.d/
cat <<EOF >/etc/NetworkManager/conf.d/99-systemd.conf
[main]
dns=default
rc-manager=resolvconf
EOF

## Deactivate systemd-resolved DNSStubListener
mkdir -p /etc/systemd/resolved.conf.d/
cat <<EOF >/etc/systemd/resolved.conf.d/99-systemd.conf
[Resolve]
DNSStubListener=no
EOF

## Fix CoreDNS timeout
mkdir -p /etc/sysconfig
echo 'RES_OPTIONS="single-request-reopen"' >>/etc/sysconfig/network

# Allow passwordless sudo command
cat <<EOF >/etc/sudoers.d/10-wheel
ENV_USERNAME ALL=(ALL) NOPASSWD: ALL
EOF
chmod 440 /etc/sudoers.d/10-wheel

# Bluetooth Configuration
systemctl enable bluetooth.service

# Graphics Configuration
add-apt-repository -y 'ppa:graphics-drivers/ppa'
dpkg --add-architecture i386
apt-get update

# Driver Configuration
## GPU - NVIDIA
if lspci | grep 'NVIDIA'; then
    # GPGPU Detection
    if lspci | grep 'NVIDIA' | grep '3D'; then
        _HAS_NVIDIA_GPGPU=true
        _HAS_NVIDIA_GPU=true
    fi

    # VGA Detection
    if lspci | grep 'NVIDIA' | grep 'VGA'; then
        _IS_DESKTOP=true
        _HAS_NVIDIA_GPU=true
        _HAS_NVIDIA_VGA=true
    fi

    if [ "x${_IS_NVIDIA_MANUAL}" == "xfalse" ]; then
        if [ "x${_HAS_NVIDIA_GPU}" == "xtrue" ]; then
            i386_packages="libnvidia-*-550:i386 libnvidia-egl-wayland1:i386"
            if [ "x$(uname -m)" != 'xx86_64' ]; then
                i386_packages=''
            fi
            apt-get install -y \
                ${i386_packages} \
                "libnvidia-*-550" \
                "libnvidia-egl-wayland1" \
                "nvidia-*-550" \
                "xserver-xorg-video-nvidia-550"
        fi

        # Enable NVIDIA FabricManager
        systemctl enable nvidia-fabricmanager.service

        # Enable NVIDIA Persistenced
        systemctl enable nvidia-persistenced.service
    fi

    # Disable Nouveau Driver
    cat <<EOF >/etc/modprobe.d/blacklist-nouveau.conf
blacklist nouveau
EOF

    # Disable GSP Firmware
    # NOTE: https://github.com/NVIDIA/dcgm-exporter/issues/84
    cat <<EOF >/etc/modprobe.d/disable-nvidia-gsp-firmware.conf
options nvidia NVreg_EnableGpuFirmware=0
EOF
fi

# ContainerD Configuration
apt-get install -y containerd podman
systemctl enable containerd.service

# Docker (Podman) Configuration
mkdir -p /etc/containers/
mkdir -p /etc/docker/
mkdir -p /etc/systemd/system/docker.service.d/
touch /etc/containers/nodocker
cat <<EOF >/etc/docker/daemon.json
{
    "insecure-registries": [
        "registry.kiss.svc.ops.openark"
    ]
}
EOF
systemctl disable podman.service
systemctl enable podman.socket

# Environment Variables Configuration
mkdir -p /etc/profile.d/
cat <<EOF >/etc/profile.d/path-local-bin.sh
# local binary path registration

export PATH=\${PATH}:/usr/local/bin
export PATH=\${PATH}:/opt/bin
EOF

# Kernel Module Configuration
mkdir -p /etc/modules-load.d/
cat <<EOF >/etc/modules-load.d/10-gpu-nvidia-driver.conf
loop
i2c_core
ipmi_msghandler
EOF

# KISS Configuration
mkdir -p /etc/systemd/system/multi-user.target.wants/
cat <<EOF >/etc/systemd/system/notify-new-box.service
[Unit]
Description=Notify to the kiss cluster that a new (this) box has been appeared.
Wants=network-online.target
After=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/notify-new-box.sh
Restart=on-failure
RestartSec=30

[Install]
WantedBy=multi-user.target
EOF
ln -sf /etc/systemd/system/notify-new-box.service /etc/systemd/system/multi-user.target.wants/notify-new-box.service

## KISS Notifier Script
cat <<EOF >/usr/local/bin/notify-new-box.sh
#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail

# Collect node info
ADDRESS="\$(ip route get 1.1.1.1 | grep -oP 'src \K\d+(\.\d+){3}' | head -1)"
UUID="\$(cat /sys/class/dmi/id/product_uuid)"

# Submit to KISS Cluster
exec curl --retry 5 --retry-delay 5 "http://gateway.kiss.svc.ops.openark/new?address=\${ADDRESS}&uuid=\${UUID}"
EOF
chmod 550 /usr/local/bin/notify-new-box.sh

# Network Configuration
mkdir -p /etc/systemd/system/multi-user.target.wants/

# Sysctl Configuration
mkdir -p /etc/sysctl.d/
cat <<EOF >/etc/sysctl.d/50-hugepages.conf
vm.nr_hugepages=0
EOF
cat <<EOF >/etc/sysctl.d/90-reverse-path-filter.conf
net.ipv4.conf.all.rp_filter=0
net.ipv4.conf.default.rp_filter=0
EOF

# User Configuration
TENANT_HOME="/opt/vdi/tenants/host"
mkdir -p "${TENANT_HOME}"
chmod 700 "${TENANT_HOME}"
if ! grep -Pq '^tenant:' /etc/passwd; then
    groupadd --gid "2000" "tenant"
    useradd --uid "2000" --gid "2000" --groups "audio,cdrom,input,pipewire,render,tty,video" \
        --home "${TENANT_HOME}" --shell "/bin/bash" \
        --non-unique "tenant"
fi
chown -R tenant:tenant "${TENANT_HOME}"

# Guest User Configuration
TENANT_GUEST_HOME="/opt/vdi/tenants/remote/guest"
mkdir -p "${TENANT_GUEST_HOME}"
chmod 700 "${TENANT_GUEST_HOME}"
chown tenant:tenant "${TENANT_GUEST_HOME}"

# Post-Hook
## Desktop Environment Configuration
if [ "x${_IS_DESKTOP}" == "xtrue" ]; then
    ### Register Mozilla PPA (Firefox)
    add-apt-repository -y ppa:mozillateam
    cat <<EOF >/etc/apt/preferences.d/mozilla
Package: *
Pin: release o=LP-PPA-mozillateam
Pin-Priority: 1000
EOF

    ### Common
    apt-get update
    apt-get install -y \
        firefox \
        firefox-locale-ko \
        mesa-vulkan-drivers \
        wireplumber \
        xdg-dbus-proxy \
        xdotool \
        x11-xserver-utils \
        xorg

    #### Enable user-level virtual console (xf86) acccess
    cat <<EOF >/etc/X11/Xwrapper.config
allowed_users=console
needs_root_rights=yes
EOF

    #### Autologin to X11
    cat <<EOF >/usr/local/bin/xinit
#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Catch trap signals
trap "echo 'Gracefully terminating...'; exit" INT TERM
trap "echo 'Terminated.'; exit" EXIT

# Configure environment variables
export DISPLAY=:0
export VINE_BASTION_ENTRYPOINT="http://bastion.vine.svc.ops.openark"

echo "Wait until graphic drivers are ready ..."
## NVIDIA
if lspci | grep 'VGA' | grep 'NVIDIA'; then
    while ! nvidia-smi >/dev/null 2>/dev/null; do
    sleep 0.1
    done
fi

echo "Starting Xorg display server ..."
/bin/Xorg "\${DISPLAY}" &
PID_DISPLAY=\$!

# Skip installation if already done
if [ -f "/tmp/.vine/.login-shell" ]; then
    exec sleep infinity
fi

# Disable screen blanking
until xset -dpms >/dev/null 2>/dev/null; do
    sleep 0.1
done
xset s off

# Get the screen size
SCREEN_WIDTH="640"
SCREEN_HEIGHT="480"

# Configure screen size
function update_screen_size() {
    echo "Finding displays..."
    screens="\$(xrandr --current | grep ' connected ' | awk '{print \$1}')"
    if [ "x\${screens}" == "x" ]; then
        echo 'Display not found!'
        exit 1
    fi

    for screen in \$(echo -en "\${screens}"); do
        echo "Fixing screen size (\${screen})..."
        until [ "\$(
            xrandr --current |
                grep ' connected' |
                grep -Po '[0-9]+x[0-9]+' |
                head -n1
        )" == "\${SCREEN_WIDTH}x\${SCREEN_HEIGHT}" ]; do
            xrandr --output "\${screen}" --mode "\${SCREEN_WIDTH}x\${SCREEN_HEIGHT}" || true
            sleep 1
        done
    done
}

# Configure firefox window
function update_window() {
    classname="\$1"

    xdotool search --classname "\${classname}" set_window --name 'Welcome'
    xdotool search --classname "\${classname}" windowsize "\${SCREEN_WIDTH}" "\${SCREEN_HEIGHT}"
    xdotool search --classname "\${classname}" windowfocus
    update_screen_size
}

update_screen_size

# Wait some times to get network connection
until curl --max-time 1 --silent "\${VINE_BASTION_ENTRYPOINT}" 2>/dev/null; do
    sleep 1
done

# Skip installation if already done
if [ -f "/tmp/.vine/.login-shell" ]; then
    exec sleep infinity
fi

echo "Executing a welcome shell..."
firefox \
    --first-startup \
    --private \
    --window-size "\${SCREEN_WIDTH},\${SCREEN_HEIGHT}" \
    --kiosk "\${VINE_BASTION_ENTRYPOINT}/print/install_os" &
PID_SHELL=\$!

echo "Waiting until window is ready..."
sleep 1
until xdotool search --classname 'Navigator' >/dev/null; do
    sleep 0.5
done

echo "Resizing window to fullscreen..."
update_window 'Navigator'

echo "Waiting until installation is succeeded..."
until [ -f "/tmp/.vine/.login-shell" ]; do
    sleep 1
done

echo "Stopping firefox..."
kill "\${PID_SHELL}" 2>/dev/null || true
exec sleep infinity
EOF
    chmod 555 /usr/local/bin/xinit

    mkdir -p /etc/profile.d/
    cat <<EOF >/etc/profile.d/x11.sh
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Execute X11 Display Server on login
if [[ "\${XDG_SESSION_TYPE}" == "tty" && "\$(id -u)" == "2000" ]]; then
    TTY="\$(tty)"
    if [[ "\${TTY/\/dev\/tty}" == "1" ]]; then
    unset TTY
    rm -rf /tmp/.vine || true
    exec /usr/local/bin/xinit
    fi
    unset TTY
fi
EOF

    #### Firefox Configuration
    mkdir -p /usr/lib/firefox/defaults/pref
    cat <<EOF >/usr/lib/firefox/defaults/pref/autoconfig.js
pref("general.config.filename", "firefox.cfg");
pref("general.config.obscure_value", 0);
EOF

    cat <<EOF >/usr/lib/firefox/firefox.cfg
// IMPORTANT: Start your code on the 2nd line

lockPref("app.update.disable_button.showUpdateHistory", true);
lockPref("browser.toolbars.bookmarks.visibility", "never");
lockPref("pref.browser.homepage.disable_button.current_page", true);
lockPref("pref.browser.homepage.disable_button.bookmark_page", true);
lockPref("pref.browser.homepage.disable_button.restore_default", true);
lockPref("pref.downloads.disable_button.edit_actions", true);
lockPref("pref.privacy.disable_button.cookie_exceptions", true);
lockPref("pref.general.disable_button.default_browser", true);
lockPref("pref.privacy.disable_button.view_cookies", true);
lockPref("pref.privacy.disable_button.view_passwords", true);
lockPref("pref.privacy.disable_button.view_passwords_exceptions", true);
lockPref("security.disable_button.openCertManager", true);
lockPref("security.disable_button.openDeviceManager", true);
lockPref("security.enterprise_roots.enabled", true);
lockPref("security.insecure_connection_icon.enabled", false);
lockPref("security.insecure_connection_icon.pbmode.enabled", false);
lockPref("security.insecure_field_warning.contextual.enabled", false);
lockPref("services.sync.prefs.sync.signon.autofillForms", false);
lockPref("signon.autofillForms", false);
lockPref("signon.autofillForms.autocompleteOff", true);
lockPref("signon.autofillForms.http", false);
lockPref("signon.showAutoCompleteFooter", false);
EOF

    ### User SystemD Configuration
    SERVICE_HOME="${TENANT_HOME}/.config/systemd/user"

    for service in \
        "pipewire.service default.target.wants/pipewire.service" \
        "pipewire.socket sockets.target.wants/pipewire.socket" \
        "pipewire-pulse.service default.target.wants/pipewire-pulse.service" \
        "pipewire-pulse.socket sockets.target.wants/pipewire-pulse.socket"; do
        SERVICE_SRC="/usr/lib/systemd/user/$(echo "${service}" | awk '{print $1}')"
        SERVICE_DST="${SERVICE_HOME}/$(echo "${service}" | awk '{print $2}')"
        if [ -f "${SERVICE_SRC}" ]; then
            mkdir -p "$(dirname "${SERVICE_DST}")"
            ln -sf "${SERVICE_SRC}" "${SERVICE_DST}"
        fi
    done
    chown -R tenant:tenant "${TENANT_HOME}"

    #### Autologin
    mkdir -p /etc/systemd/system/getty@tty1.service.d/
    cat <<EOF >/etc/systemd/system/getty@tty1.service.d/override.conf
[Service]
ExecStart=
ExecStart=-/sbin/agetty -a tenant --noclear - $TERM
EOF

    #### Limit the maximum number of TTYs to 1
    _LOGIND="/etc/systemd/logind.conf"
    sed -i 's/^\#\?\(NAutoVTs=\).*$/\11/g' "${_LOGIND}"
    sed -i 's/^\#\?\(ReserveVT=\).*$/\11/g' "${_LOGIND}"
    for i in {2..63}; do
        systemctl mask getty@tty${i}.service >/dev/null
    done

    #### Disable VT Switching
    mkdir -p /etc/X11/xorg.conf.d/
    cat <<EOF >/etc/X11/xorg.conf.d/65-setxkbmap.conf
Section "ServerFlags"
    Option "DontVTSwitch" "on"
EndSection

Section "InputClass"
    Identifier "keyboard defaults"
    MatchIsKeyboard "on"
    Option "XKbOptions" "srvrkeys:none"
EndSection
EOF

    #### Disable Screen Blank Time
    cat <<EOF >/etc/X11/xorg.conf.d/10-monitor.conf
Section "ServerFlags"
    Option "BlankTime" "0"
    Option "OffTime" "0"
    Option "StandbyTime" "0"
    Option "SuspendTime" "0"
EndSection
EOF
fi

# DKMS Build
if which dkms >/dev/null 2>/dev/null; then
    SRC_KERNEL_VERSION="$(ls '/lib/modules/' | sort -V | tail -n1)"
    dkms autoinstall -k "${SRC_KERNEL_VERSION}"
fi

# Kernel Command-line
## VFIO
#grubby --update-kernel=ALL --args='amd_iommu=on'
#grubby --update-kernel=ALL --args='intel_iommu=on'
#grubby --update-kernel=ALL --args='iommu=pt'

## Kernel
#VMLINUZ_KERNEL_PATH="$(find /boot -maxdepth 1 -name 'vmlinuz-*' | sort -V | tail -n1)"
#grubby --set-default="${VMLINUZ_KERNEL_PATH}"

## Apply
grub-mkconfig -o /boot/grub/grub.cfg

# Boot Order
BOOT_NUM="$(
    efibootmgr |
        grep -P '^Boot[0-9A-F]+(\*)?[ \t]+Ubuntu[ \t]+' |
        grep -P '\\EFI\\ubuntu\\shimx64.efi\)$' |
        grep -Po '^Boot\K[0-9A-F]+'
)"
#efibootmgr --bootnum "${BOOT_NUM}" --active
efibootmgr --bootorder "${BOOT_NUM}"

# Hostname
UUID="$(cat /sys/class/dmi/id/product_uuid)"
echo "127.0.0.1 ${UUID}" >>/etc/hosts
echo -n "${UUID}" >/etc/hostname

# VFIO
echo 'vfio-pci' >/etc/modules-load.d/vfio-pci.conf
