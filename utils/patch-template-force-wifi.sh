#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

set -x

ethernet_name="$(ip a | grep enp | awk '{print $2}' | grep -Po '^en[a-z0-9]+' | tail -n1)"
wlan_name="$(ip a | grep wlp | awk '{print $2}' | grep -Po '^wl[a-z0-9]+' | tail -n1)"

if [ "x${ethernet_name}" = 'xmaster' ]; then
    ethernet_name="$(ip a show dev "${ethernet_name}" | tail -n2 | head -n1 | awk '{print $8}')"
fi
if [ "x${wlan_name}" = 'xmaster' ]; then
    wlan_name="$(ip a show dev "${wlan_name}" | tail -n2 | head -n1 | awk '{print $8}')"
fi

if [ "x${ethernet_name}" != 'x' ] && [ "x${wlan_name}" != 'x' ]; then
    ethernet_mac="$(ip a show dev "${ethernet_name}" | head -n2 | tail -n1 | awk '{print $2}')"
    wlan_mac="$(ip a show dev "${wlan_name}" | head -n2 | tail -n1 | awk '{print $2}')"

    if [ "x${wlan_mac}" != 'x' ]; then
        cd /etc/udev/rules.d/
        udev_dst="./70-kiss-net-setup-link-master.rules"

        udev_master="./70-kiss-net-setup-link-${ethernet_name}.rules"
        if [ -f "${udev_master}" ]; then
            sudo mv "${udev_master}" "${udev_dst}"
        fi
        sudo sed -i "s/${ethernet_mac}/${wlan_mac}/g" "${udev_dst}"
        sudo sed -i "s/${ethernet_name}/master/g" "${udev_dst}"

        cd /etc/NetworkManager/system-connections/
        enable_dst="./10-kiss-enable-master.nmconnection"
        disable_src="./20-kiss-disable-${wlan_name}.nmconnection"
        disable_dst="./20-kiss-disable-${ethernet_name}.nmconnection"

        enable_master="./10-kiss-enable-${ethernet_name}.nmconnection"
        if [ -f "${enable_master}" ]; then
            sudo mv "${enable_master}" "${enable_dst}"
        fi

        disable_master='./20-kiss-disable-master.nmconnection'
        if [ -f "${disable_master}" ]; then
            if ip a show dev "${wlan_name}"; then
                sudo mv "${disable_master}" "${disable_src}"
            fi
        fi

        if [ -f "${disable_src}" ]; then
            sudo mv "${disable_src}" "${disable_dst}"
            sudo sed -i "s/\(disable-\)[0-9a-z]\+/\1${ethernet_name}/g" "${disable_dst}"
            sudo sed -i "s/\=wifi/\=ethernet/g" "${disable_dst}"
            sudo sed -i "s/${wlan_name}/${ethernet_name}/g" "${disable_dst}"
            sudo sed -i "s/\(mac-address=\)[0-9a-f:]\+/\1${ethernet_mac}/g" "${disable_dst}"
            sudo sed -i "s/\(qdisc\.root=\)[_a-z]*/\1mq/g" "${disable_dst}"

            sudo sed -i "s/\(enable-\)[0-9a-z]\+/\1master/g" "${enable_dst}"
            sudo sed -i "s/\=ethernet/\=wifi/g" "${enable_dst}"
            sudo sed -i "s/${ethernet_name}/${wlan_name}/g" "${enable_dst}"
            sudo sed -i "s/\(mac-address=\)[0-9a-f:]\+/\1${wlan_mac}/g" "${enable_dst}"
            sudo sed -i "s/\(qdisc\.root=\)[_a-z]*/\1noqueue/g" "${enable_dst}"

            sudo nmcli connection reload &&
                sudo nmcli connection up '10-kiss-enable-master'
            sudo reboot
        fi
    fi
fi
