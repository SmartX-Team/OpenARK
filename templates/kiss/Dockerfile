# Configure environment variables
ARG ALPINE_VERSION="latest"
ARG K8S_VERSION="latest"

# Be ready for serving
FROM docker.io/alpine:${ALPINE_VERSION} as server

# Server Configuration
WORKDIR /src
ENTRYPOINT [ "/bin/sh" ]
CMD [ "./install.sh" ]

# Be ready for building
FROM docker.io/alpine:${ALPINE_VERSION} as builder

# Install dependencies
RUN apk add --no-cache curl

# Configure environment variables
ARG K8S_VERSION=${K8S_VERSION}

# Download binary
RUN curl -LO "https://dl.k8s.io/release/v${K8S_VERSION}/bin/linux/amd64/kubectl" \
    && mv kubectl /usr/local/bin/ \
    && chmod a+x /usr/local/bin/kubectl

# Copy executable files
FROM server
ADD . /src/
COPY --from=builder /usr/local/bin/kubectl /usr/local/bin/kubectl
