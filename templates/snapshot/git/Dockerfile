# Configure environment variables
ARG ALPINE_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
WORKDIR /root
CMD [ "/usr/local/bin/upload.sh" ]

# Install dependencies
RUN apk add --no-cache git

# Load executable files
COPY ./upload.sh /usr/local/bin/
