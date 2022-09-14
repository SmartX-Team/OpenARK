# Configure environment variables
ARG ROCKYLINUX_VERSION="latest"

# Be ready for serving
FROM quay.io/rockylinux/rockylinux:${ROCKYLINUX_VERSION} as server

# Server Configuration
VOLUME [ "/sys/fs/cgroup" ]
WORKDIR /root/
CMD ["/usr/local/bin/init.sh"]

# Remove a number of unit files which might cause issues
# note: https://hub.docker.com/r/rockylinux/rockylinux
RUN pushd /lib/systemd/system/sysinit.target.wants/ && for i in *; do \
    [ $i == systemd-tmpfiles-setup.service ] || rm -f $i; \
    done && popd \
    && rm -f /lib/systemd/system/multi-user.target.wants/* \
    && rm -f /etc/systemd/system/*.wants/* \
    && rm -f /lib/systemd/system/local-fs.target.wants/* \
    && rm -f /lib/systemd/system/sockets.target.wants/*udev* \
    && rm -f /lib/systemd/system/sockets.target.wants/*initctl* \
    && rm -f /lib/systemd/system/basic.target.wants/* \
    && rm -f /lib/systemd/system/anaconda.target.wants/*

# Install dependencies
RUN yum install -y \
    ethtool findutils iproute iproute-tc kmod \
    openssh-clients openssh-server \
    procps-ng python3 \
    # Cleanup
    && yum clean all \
    && rm -rf /var/cache/yum

# Systemd Configuration
RUN true \
    # Configure sshd
    && sed -i 's/^#\(ListenAddress\) 0\.0\.0\.0/\1 127.0.0.1/g' /etc/ssh/sshd_config \
    && sed -i 's/^\(PasswordAuthentication\) yes/\1 no/g' /etc/ssh/sshd_config \
    # Enable systemd
    && systemctl enable sshd

# Load files
ADD ./ansible/ /etc/kiss/bootstrap/
ADD ./init.sh /usr/local/bin/init.sh

# Change permissions of given files
RUN chmod 500 /usr/local/bin/init.sh
