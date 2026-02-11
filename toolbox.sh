#!/usr/bin/env bash

[ -f '/run/.toolboxenv' ] || {
    echo >&2 'Not inside a toolbox environment.';
    exit 1;
}

sudo dnf install --assumeyes \
    'gcc' \
    'musl-gcc' \
    'xz-devel'
