# Configure environment variables
ARG K8S_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine/k8s:${K8S_VERSION} as server

# Server Configuration
WORKDIR /src
ENTRYPOINT [ "/bin/bash" ]
CMD [ "./upload.sh" ]

# Copy executable files
ADD ./upload.sh /src/
