# Configure environment variables
ARG K8S_VERSION

# Be ready for serving
FROM docker.io/bitnami/kubectl:${K8S_VERSION} as server

# Load source files
ADD . /src/

# Job Configuration
WORKDIR /src
ENTRYPOINT [ "/bin/sh" ]
CMD [ "./install.sh" ]
