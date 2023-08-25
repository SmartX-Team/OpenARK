sudo systemctl disable --now optimize-wifi.service

sudo chmod u+w /etc/NetworkManager/system-connections/10-kiss-enable-master.nmconnection || true
sudo sed -i 's/bssid/#\0/g' /etc/NetworkManager/system-connections/10-kiss-enable-master.nmconnection || true
sudo chmod u-w /etc/NetworkManager/system-connections/10-kiss-enable-master.nmconnection || true
sudo systemctl restart NetworkManager.service
