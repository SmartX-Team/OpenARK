# Configure environment variables
ARG ROCKYLINUX_VERSION="latest"

# Be ready for serving
FROM quay.io/rockylinux/rockylinux:${ROCKYLINUX_VERSION} as server

# SystemD Configuration
ENV container docker
STOPSIGNAL SIGRTMIN+3
VOLUME [ "/sys/fs/cgroup" ]
VOLUME [ "/var/lib/containerd" ]

# Server Configuration
WORKDIR /root/
CMD ["/usr/local/bin/entrypoint.sh"]

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
    # Ansible & Core
    ethtool findutils iproute iproute-tc kmod \
    openssh-clients openssh-server \
    procps-ng python3 \
    # Kubernetes
    bash-completion conntrack container-selinux curl \
    device-mapper-libs ebtables e2fsprogs \
    libseccomp libselinux-python3 \
    openssl rsync socat tar unzip xfsprogs \
    # Cleanup
    && yum clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Systemd Configuration
RUN true \
    # Configure sshd
    && sed -i 's/^#\(ListenAddress\) 0\.0\.0\.0/\1 127.0.0.1/g' /etc/ssh/sshd_config \
    && sed -i 's/^\(PasswordAuthentication\) yes/\1 no/g' /etc/ssh/sshd_config \
    # Enable systemd
    && systemctl enable sshd

# Load files
ADD ./ansible/ /etc/kiss/bootstrap/
ADD ./entrypoint.sh /usr/local/bin/entrypoint.sh

# Change permissions of given files
RUN chmod 500 /usr/local/bin/entrypoint.sh
