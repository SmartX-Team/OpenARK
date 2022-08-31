# Configure environment variables
ARG \
    K8S_VERSION="1.24" \
    PACKAGE="netai-cloud-upgrade-kiss"

# Be ready for serving
FROM docker.io/bitnami/kubectl:${K8S_VERSION} as server

# Metadata Configuration
LABEL \
    AUTHOR="kerryeon" \
    K8S_VERSION=${K8S_VERSION} \
    LICENSE="GPL v3.0 with a classpath linking exception" \
    PACKAGE=${PACKAGE}

# Load source files
ADD . /src/

# Job Configuration
WORKDIR /src
ENTRYPOINT [ "/bin/sh" ]
CMD [ "./install.sh" ]
