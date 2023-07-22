# Disable Power Saving Mode
cat <<EOF | sudo tee /etc/modprobe.d/iwlwifi.conf
options iwlmvm power_scheme=1
options iwlwifi power_save=0
EOF

# Disable Power Saving Mode on NetworkManager
sudo mkdir -p /etc/NetworkManager/conf.d/
cat <<EOF | sudo tee /etc/NetworkManager/conf.d/default-wifi-powersave-off.conf
[connection]
wifi.powersave = 2
EOF

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
