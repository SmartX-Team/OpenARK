#!/bin/bash
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

###########################################################
#   Autocompletion - COMMON                               #
###########################################################

function _box_get_autocomplete() {
    local cur="${COMP_WORDS[COMP_CWORD]}"

    if [ "x${COMP_CWORD}" = 'x1' ]; then
        COMPREPLY=($(compgen -W "$(
            kubectl get box \
                --output jsonpath \
                --template '{.items[*].metadata.labels.dash\.ulagbulag\.io/alias}'
        )" -- "${cur}"))
    fi
}

###########################################################
#   Autocompletion - box-ls                               #
###########################################################

complete -F _box_get_autocomplete 'box-ls'

###########################################################
#   Autocompletion - box-ssh                              #
###########################################################

complete -F _box_get_autocomplete 'box-ssh'
