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

# Restart if updated
if ip a | grep wlp >/dev/null 2>/dev/null; then
    if [ "x${REBOOT}" = 'xy' ]; then
        sudo reboot
    fi
fi
