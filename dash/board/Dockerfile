# Copyright (c) 2022 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Configure environment variables
ARG ALPINE_VERSION="latest"
ARG PACKAGE="openark"

# Be ready for serving
FROM "docker.io/library/alpine:${ALPINE_VERSION}"

# Server Configuration
EXPOSE 80/tcp
WORKDIR /src
CMD [ "streamlit", "run", "0_Home.py", "--server.address=0.0.0.0", "--server.port=80", "--browser.gatherUsageStats=False" ]

# Install dependencies
RUN apk add --no-cache \
    cython \
    # py3-altair \
    py3-blinker \
    py3-cachetools \
    py3-certifi \
    py3-charset-normalizer \
    py3-click \
    py3-entrypoints \
    py3-gitdb2 \
    py3-gitpython \
    py3-idna \
    py3-importlib-metadata \
    py3-inflect \
    py3-jinja2 \
    py3-jsonschema \
    py3-markdown-it-py \
    py3-numpy \
    py3-packaging \
    py3-pandas \
    py3-pillow \
    py3-pip \
    # py3-protobuf \
    py3-pyarrow \
    # py3-pydeck \
    py3-pygments \
    # py3-pympler \
    py3-requests \
    py3-rich \
    py3-scikit-learn \
    py3-semver \
    py3-setuptools \
    py3-setuptools_scm \
    py3-toml \
    py3-tomli \
    py3-toolz \
    py3-tornado \
    py3-typing-extensions \
    py3-tzlocal \
    py3-urllib3 \
    py3-validators \
    py3-watchdog \
    py3-wheel

# Add dependencies file
ADD ./requirements.txt /src/requirements.txt

# Install python dependencies
RUN python3 -m pip install --no-cache-dir --requirement ./requirements.txt \
    # Cleanup
    && find /usr -type d -name '*__pycache__' -prune -exec rm -rf {} \;

# Add source code
ADD . /src
