#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

# Prehibit errors
set -e -o pipefail
# Verbose
set -x

# Remove Google Chrome singletons
rm -rf \
    "${HOME}/.config/google-chrome/SingletonCookie" \
    "${HOME}/.config/google-chrome/SingletonLock" \
    "${HOME}/.config/google-chrome/SingletonSocket"
