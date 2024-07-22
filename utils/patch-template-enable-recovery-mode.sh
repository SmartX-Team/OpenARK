#!/bin/bash
# Copyright (c) 2024 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

set -x

sudo systemctl disable notify-new-box
sudo rm -rf /etc/udev/rules.d/70-kiss-*.rules

echo 'blacklist i915' >/etc/modprobe.d/blacklist-i915.conf
echo 'blacklist snd_hda_intel' >>/etc/modprobe.d/blacklist-i915.conf

sudo sed -i 's/tenant/kiss/g' /etc/systemd/system/getty@tty1.service.d/override.conf
sudo reboot
