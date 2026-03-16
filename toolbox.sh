#!/usr/bin/env bash

[ -f '/run/.toolboxenv' ] || {
    echo >&2 'Not inside a toolbox environment.';
    exit 1;
}

# CLang required for Bzip3.
# Musl plugin required for `cargo deploy` alias.
sudo dnf install --assumeyes \
    'clang-devel' \
    'gcc' \
    'musl-gcc' \
    'xz-devel'
