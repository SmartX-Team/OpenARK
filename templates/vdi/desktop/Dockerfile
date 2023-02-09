# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG CUDA_VERSION="12.0.1"
ARG ROCKYLINUX_VERSION_MAJOR="8"

# Configure user-specific environment variables
ARG USER_GID="2000"
ARG USER_NAME="user"
ARG USER_SHELL="zsh"
ARG USER_SUDO="true"
ARG USER_UID="2000"

# Be ready for serving
FROM "docker.io/nvidia/cuda:${CUDA_VERSION}-runtime-rockylinux${ROCKYLINUX_VERSION_MAJOR}"

# Load environment variables
ARG ROCKYLINUX_VERSION_MAJOR

# Load user-specific environment variables
ARG USER_GID
ARG USER_NAME
ARG USER_SHELL
ARG USER_SUDO
ARG USER_UID

# SystemD Configuration
ENV container docker
STOPSIGNAL SIGRTMIN+3
VOLUME [ "/sys/fs/cgroup" ]

# Client Configuration
WORKDIR /root/
ENTRYPOINT ["/usr/bin/env"]
CMD ["/usr/bin/firefox"]

# Volumes Configuration
## home.user
VOLUME [ "/home/${USER_NAME}" ]

# Update locale
RUN echo 'override_install_langs=en_US' >>/etc/yum.conf

# Add core repositories
ADD ./repos/*.repo /etc/yum.repos.d/
RUN dnf install -y \
    epel-release \
    "https://pkgs.dyn.su/el${ROCKYLINUX_VERSION_MAJOR}/base/x86_64/raven-release.el${ROCKYLINUX_VERSION_MAJOR}.noarch.rpm" \
    # Cleanup
    && dnf clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Install core dependencies
RUN dnf install -y \
    libglvnd-gles \
    mesa-dri-drivers \
    pipewire \
    vulkan \
    # wireplumber \
    xdg-dbus-proxy \
    # Cleanup
    && dnf clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Install development environment dependencies
RUN dnf install -y \
    "@Development Tools" \
    git \
    nano \
    pciutils \
    podman-docker \
    python3 \
    python3-pip \
    sudo \
    "${USER_SHELL}" \
    vim \
    wget \
    yum-utils \
    zip \
    # Allow passwordless sudo command
    && if [ "${USER_SUDO}" == "true" ]; then \
    echo "${USER_NAME} ALL=(ALL) NOPASSWD: ALL" >/etc/sudoers.d/10-wheel \
    && chmod 440 /etc/sudoers.d/10-wheel \
    ; fi \
    # Docker (Podman) Configuration
    && systemctl enable podman \
    # Environment Variables Configuration
    && echo '# local binary path registration' >/etc/profile.d/path-local-bin.sh \
    && echo 'export PATH=${PATH}:/usr/local/bin' >/etc/profile.d/path-local-bin.sh \
    && echo 'export PATH=${PATH}:/opt/bin' >/etc/profile.d/path-local-bin.sh \
    && ln -sf /usr/local/bin /opt/bin \
    # Cleanup
    && dnf clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Install desktop environment dependencies
RUN dnf install -y \
    code \
    dbus-x11 \
    file-roller \
    ibus-hangul \
    network-manager-applet \
    openssh-askpass \
    picom \
    plank \
    sqlite \
    Thunar thunar-archive-plugin thunar-volman \
    tumbler \
    xfce4-appfinder xfce4-panel xfce4-pulseaudio-plugin xfce4-session \
    xfce4-settings xfce4-terminal \
    xfconf xfdesktop xfwm4 \
    # Cleanup
    && dnf clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Install internet browsers
RUN dnf install -y \
    firefox \
    google-chrome-stable \
    # Cleanup
    && dnf clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Install locale dependencies
RUN dnf install -y \
    glibc-langpack-ko \
    # Cleanup
    && dnf clean all \
    && rm -rf /var/cache /var/log/dnf* /var/log/yum.*

# Add a user
RUN groupadd -g "${USER_GID}" -o "${USER_NAME}" \
    && useradd -u "${USER_UID}" -g "${USER_GID}" -G "audio,cdrom,input,pipewire,render,video" \
    -s "/bin/${USER_SHELL}" -m -o "${USER_NAME}"
USER "${USER_NAME}"
WORKDIR "/home/${USER_NAME}"
