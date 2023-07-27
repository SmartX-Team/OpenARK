# Disable Power Saving Mode
if ! ls /etc/modprobe.d/iwlwifi.conf >/dev/null 2>/dev/null; then
    REBOOT='y'
fi
cat <<EOF | sudo tee /etc/modprobe.d/iwlwifi.conf
options iwlmvm power_scheme=1
options iwlwifi power_save=0 disable_11ax=1
EOF

# Disable Power Saving Mode on NetworkManager
if ! ls /etc/NetworkManager/conf.d/default-wifi-powersave-off.conf >/dev/null 2>/dev/null; then
    REBOOT='y'
fi
sudo mkdir -p /etc/NetworkManager/conf.d/
cat <<EOF | sudo tee /etc/NetworkManager/conf.d/default-wifi-powersave-off.conf
[connection]
wifi.powersave = 2
EOF

# Install driver: iwlwifi-killer-ax1690-7af0
if sudo dmesg | grep iwlwifi | grep -P '7af0/[0-9]+' >/dev/null 2>/dev/null; then
    if ! ip a | grep wlp >/dev/null 2>/dev/null; then
        REBOOT='y'
        sudo dnf remove -y kernel-headers
        sudo dnf --enablerepo=elrepo-kernel install -y \
            kernel-ml \
            kernel-ml-core \
            kernel-ml-devel \
            kernel-ml-modules \
            kernel-ml-modules-extra \
            xorg-x11-server-Xorg

        if lspci | grep 'NVIDIA'; then
            NVIDIA_DRIVER_VERSION="$(
                dnf list kmod-nvidia-latest-dkms |
                    awk '{print $2}' |
                    grep -Po '[0-9]+\.[0-9]+\.[0-9]+'
            )"
            INSTALLER_FILE="./installer.run"
            INSTALLER_SRC="https://us.download.nvidia.com/XFree86/Linux-$(uname -m)/${NVIDIA_DRIVER_VERSION}/NVIDIA-Linux-$(uname -m)-${NVIDIA_DRIVER_VERSION}.run"
            wget -O "${INSTALLER_FILE}" "${INSTALLER_SRC}"
            sudo killall Xorg || true
            sudo rmmod nvidia_drm || true
            sudo rmmod nvidia_modeset || true
            sudo rmmod nvidia_uvm || true
            sudo rmmod nvidia || true
            sudo bash ./installer.run --accept-license --dkms --silent --systemd
            rm -f "${INSTALLER_FILE}"
        fi

        SRC_KERNEL_VERSION="$(ls '/lib/modules/' | sort | tail -n1)"
        sudo dkms autoinstall -k "${SRC_KERNEL_VERSION}"
    fi
fi

# Install driver: rtl8188eu
if ip a | grep wlp >/dev/null 2>/dev/null; then
    sudo dnf install -y @development bc dkms git vim
    SRC_DRIVER="rtl8188eus"
    SRC_HOME="/usr/src/${SRC_DRIVER}"
    SRC_REPO="https://github.com/ulagbulag/${SRC_DRIVER}.git"

    if ! modprobe 8188eu; then
        sudo rm -rf $SRC_HOME*
    fi
    if ! ls $SRC_HOME* >/dev/null 2>/dev/null; then
        sudo dnf install -y \
            kernel \
            kernel-core \
            kernel-devel \
            kernel-headers \
            kernel-modules \
            kernel-modules-core
        sudo git clone "${SRC_REPO}" "${SRC_HOME}"
        pushd "${SRC_HOME}"
        SRC_VERSION="$(git branch | awk '{print $2}' | grep -Po '^v\K.*')"
        SRC_HOME_VERSION="${SRC_HOME}-${SRC_VERSION}"
        sudo mv "${SRC_HOME}" "${SRC_HOME_VERSION}"
        SRC_KERNEL_VERSION="$(ls '/lib/modules/' | sort | tail -n1)"
        sudo dkms add -m "${SRC_DRIVER}" -v "${SRC_VERSION}" -k "${SRC_KERNEL_VERSION}"
        sudo dkms build -m "${SRC_DRIVER}" -v "${SRC_VERSION}" -k "${SRC_KERNEL_VERSION}"
        sudo dkms install -m "${SRC_DRIVER}" -v "${SRC_VERSION}" -k "${SRC_KERNEL_VERSION}"
        sudo reboot
    fi
fi

# Restart if updated
if ip a | grep wlp >/dev/null 2>/dev/null; then
    if [ "x${REBOOT}" = 'xy' ]; then
        sudo reboot
    fi
fi
