#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

set -e -x -o pipefail

ethernet_name="$(ip a | grep enp | awk '{print $2}' | grep -Po '^en[a-z0-9]+' | tail -n1)"
wlan_name="$(ip a | grep wlp | awk '{print $2}' | grep -Po '^wl[a-z0-9]+' | tail -n1)"

if [ "x${wlan_name}" != 'x' ]; then
    ethernet_mac="$(ip a show dev "${ethernet_name}" | head -n2 | tail -n1 | awk '{print $2}')"
    wlan_mac="$(ip a show dev "${wlan_name}" | head -n2 | tail -n1 | awk '{print $2}')"

    cd /etc/udev/rules.d/
    udev_dst="./70-kiss-net-setup-link-master.rules"
    sudo find . -name '70-kiss-net-setup-link-*' -exec mv '{}' "${udev_dst}" \; || true
    sudo chmod u+w "${udev_dst}"
    sudo sed -i "s/${wlan_mac}/${ethernet_mac}/g" "${udev_dst}"
    sudo sed -i "s/NAME=\"[0-9a-z]*\"/NAME=\"master\"/g" "${udev_dst}"
    sudo chmod u-w "${udev_dst}"

    cd /etc/NetworkManager/system-connections/
    enable_dst="./10-kiss-enable-master.nmconnection"
    sudo find . -name '10-kiss-enable-*' -exec mv '{}' "${enable_dst}" \; || true
    disable_src="./20-kiss-disable-${ethernet_name}.nmconnection"
    disable_dst="./20-kiss-disable-${wlan_name}.nmconnection"
    if [ -f "${disable_src}" ]; then
        sudo mv "${disable_src}" "${disable_dst}"
        sudo chmod u+w "${disable_dst}"
        sudo sed -i "s/^id=20-kiss-disable-[0-9a-z]*$/id=20-kiss-disable-${wlan_name}/g" "${disable_dst}"
        sudo sed -i "s/\=ethernet/\=wifi/g" "${disable_dst}"
        sudo sed -i "s/${ethernet_name}/${wlan_name}/g" "${disable_dst}"
        sudo sed -i "s/${ethernet_mac}/${wlan_mac}/g" "${disable_dst}"
        sudo chmod u-w "${disable_dst}"

        sudo chmod u+w "${enable_dst}"
        sudo sed -i "s/^id=10-kiss-enable-[0-9a-z]*$/id=10-kiss-enable-master/g" "${enable_dst}"
        sudo sed -i "s/\=wifi/\=ethernet/g" "${enable_dst}"
        sudo sed -i "s/${wlan_name}/${ethernet_name}/g" "${enable_dst}"
        sudo sed -i "s/${wlan_mac}/${ethernet_mac}/g" "${enable_dst}"
        sudo chmod u-w "${enable_dst}"
    fi

    sudo reboot
fi
