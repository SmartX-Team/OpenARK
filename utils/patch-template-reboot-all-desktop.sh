#!/bin/bash

# Reboot all Desktops
if ip a | grep wlp >/dev/null 2>/dev/null; then
    sudo reboot
fi
