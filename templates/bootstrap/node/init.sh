#!/bin/bash

# Prehibit errors
set -e

# Port scanner
function get_available_port() {
    comm -23 <(seq 49152 65535 | sort) <(ss -Htan | awk '{print $4}' | cut -d':' -f2 | sort -u) | shuf | head -n 1
}

# Generate SSH keys
ssh-keygen -q -t rsa -f $HOME/.ssh/id_rsa -N ''
ssh-keygen -q -A

# Register the given public SSH key as authorized
if [ ! "$SSH_PUBKEY" ]; then
    echo "Error: SSH Public Key (\$SSH_PUBKEY) is not given!"
    exit 1
fi
echo $SSH_PUBKEY >>$HOME/.ssh/authorized_keys
chmod 600 $HOME/.ssh/authorized_keys

# Find an available SSH port
while [ ! "$ssh_port" ]; do
    ssh_port=$(get_available_port)
done

# Apply the SSH port
sed -i "s/^#\(Port\) 22/\1 ${ssh_port}/g" /etc/ssh/sshd_config

# Replace /etc/hostname to local
cp /etc/hostname /tmp/hostname &&
    umount /etc/hostname &&
    mv /tmp/hostname /etc/hostname

# Replace /etc/hosts to local
cp /etc/hosts /tmp/hosts &&
    umount /etc/hosts &&
    mv /tmp/hosts /etc/hosts

# Execute systemd
exec /usr/sbin/init
