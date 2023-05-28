#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Cleanup all unused disks.
# It is compatiable with Ceph OSD.

# Prehibit errors
set -e -o pipefail

###########################################################
#   Cleanup Devices                                       #
###########################################################

# Find real disks
for disk in $(
    find /dev/disk/by-id -type l -print0 |
        xargs -0 realpath |
        sort |
        uniq
); do
    # Skip if mounted partiton
    if findmnt -S "${disk}" >/dev/null 2>/dev/null; then
        echo "Skipping mounted partition: ${disk}"
        continue
    fi

    # Skip if mounted disk
    if [ "$(
        lsblk --noheadings "${disk}" 2>/dev/null |
            wc -l
    )" != "1" ]; then
        echo "Skipping mounted disk: ${disk}"
        continue
    fi

    # Skip if empty disk
    if [ "$(
        lsblk --bytes --noheadings --output 'SIZE' "${disk}" 2>/dev/null |
            awk '{print $1}'
    )" == "0" ]; then
        echo "Skipping empty disk: ${disk}"
        continue
    fi

    # Wipe all data
    echo "Wiping all: ${disk}"

    ## Wipe Filesystem
    wipefs --all --force "${disk}" && sync

    ## Wipe GUID partiton table (GPT)
    sgdisk --zap-all "${disk}" && sync

    ## Fill with zero to Erase metadata (1Gi)
    dd if=/dev/zero of="${disk}" bs=1M count=1024 && sync || true

    ## Discard sectors
    blkdiscard --force "${disk}" && sync || true

    ## Inform the OS of partition table changes
    partprobe "${disk}" && sync
done
