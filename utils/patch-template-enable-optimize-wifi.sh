#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

sudo systemctl enable --now optimize-wifi.service

sudo chmod u+w /etc/NetworkManager/system-connections/10-kiss-enable-master.nmconnection || true
sudo sed -i 's/bssid/#\0/g' /etc/NetworkManager/system-connections/10-kiss-enable-master.nmconnection || true
sudo chmod u-w /etc/NetworkManager/system-connections/10-kiss-enable-master.nmconnection || true
sudo systemctl restart NetworkManager.service
