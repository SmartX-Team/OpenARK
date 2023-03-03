# wsmancli BuildTool

This directory contains templates related to the `wsmanhelper.sh` script, 
a command helper tool similar in usage to `IPMITOOL` .

## Usage

```bash
# Download the helper image
docker pull "quay.io/ulagbulag-village/netai-cloud-wsmancli:latest"

# Execute your own query
docker run --rm --name netai-cloud-wsmancli \
    --env AMT_HOSTNAME="your machine AMT IP" \
    --env AMT_USERNAME="your machine AMT Username (Default: admin)" \
    --env AMT_PASSWORD="your machine AMT Password" \
    "quay.io/ulagbulag-village/netai-cloud-wsmancli:latest" \
    power on
```

### Available Queries

* Boot Device Management
    - boot cd
    - boot disk
    - boot pxe
* Power Management
    - power cycle
    - power hibernate
    - power off
    - power on
    - power reboot
    - power standby
