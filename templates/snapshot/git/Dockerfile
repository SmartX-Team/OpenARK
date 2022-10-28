# Configure environment variables
ARG K8S_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine/k8s:${K8S_VERSION} as server

# Server Configuration
WORKDIR /src
ENTRYPOINT [ "/bin/bash" ]
CMD [ "./upload.sh" ]

# Install dependencies
RUN apk add --no-cache openssh

# Copy executable files
ADD ./upload.sh /src/

# Move ssh configuration files
ADD ./ssh.config /root/.ssh/config

# Change configuration files' permissions
RUN chmod 400 /root/.ssh/config
