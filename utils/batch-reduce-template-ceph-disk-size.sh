#!/bin/bash

exec echo "$(
    lsblk -bdno NAME,FSTYPE,SIZE,TYPE |
        grep -P ' ceph_bluestore ' |
        awk '{print $3}' |
        paste -sd+ - |
        bc
)"
