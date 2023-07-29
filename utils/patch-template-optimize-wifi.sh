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

        sudo rpm --import "https://www.elrepo.org/RPM-GPG-KEY-elrepo.org"
        sudo dnf install -y \
            "https://www.elrepo.org/elrepo-release-$(rpm -E %rhel).el$(rpm -E %rhel).elrepo.noarch.rpm"
        sudo dnf --enablerepo=elrepo-kernel install -y \
            dnf-plugin-nvidia \
            egl-wayland \
            kernel-ml \
            kernel-ml-core \
            kernel-ml-devel \
            kernel-ml-modules \
            kernel-ml-modules-extra \
            libX11-devel \
            libX11-xcb \
            libXau-devel \
            libXdmcp \
            libXfont2 \
            libXtst \
            libdrm \
            libepoxy \
            libevdev \
            libglvnd \
            libglvnd-devel \
            libglvnd-egl \
            libglvnd-gles \
            libglvnd-glx \
            libglvnd-opengl \
            libinput \
            libpciaccess \
            libtirpc \
            libvdpau \
            libxcb-devel \
            mesa-libEGL \
            mesa-libGL \
            mesa-libgbm \
            mesa-libglapi \
            mesa-vulkan-drivers \
            ocl-icd \
            opencl-filesystem \
            vulkan-loader \
            wget \
            xorg-x11-drv-fbdev \
            xorg-x11-drv-libinput \
            xorg-x11-proto-devel \
            xorg-x11-server-Xorg \
            xorg-x11-server-common

        if lspci | grep 'NVIDIA'; then
            ## Desktop Environment Configuration
            if [ "$(uname -m)" = 'x86_64' ]; then
                ARCH_WIN32='i686'
            else
                ARCH_WIN32="$(uname -m)"
            fi

            sudo dnf module install -y "nvidia-driver:latest-dkms"
            sudo dnf install -y \
                cuda \
                "mesa-dri-drivers.${ARCH_WIN32}" \
                "mesa-libGLU.${ARCH_WIN32}" \
                "nvidia-driver-cuda-libs.${ARCH_WIN32}" \
                "nvidia-fabric-manager" \
                "nvidia-driver-libs.${ARCH_WIN32}" \
                "nvidia-driver-NvFBCOpenGL.${ARCH_WIN32}" \
                "nvidia-driver-NVML.${ARCH_WIN32}" \
                vulkan

            # Enable NVIDIA FabricManager
            sudo systemctl enable nvidia-fabricmanager.service

            # Enable NVIDIA Persistenced
            sudo systemctl enable nvidia-persistenced.service
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
