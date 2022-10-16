# Configure environment variables
ARG ALPINE_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
WORKDIR /ipxe
ENTRYPOINT [ "/usr/sbin/ipmitool" ]
CMD [ "" ]

# Install dependencies
RUN apk add --no-cache ipmitool
