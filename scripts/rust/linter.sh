#!/usr/bin/env bash

set -euo pipefail

FMT_OPTS=${FMT_OPTS:-"--check"}

for d in `find -maxdepth 2 -name Cargo.toml -printf '%h\n' | grep -v "^./h2" | grep -v "git-version-macro"`; do
    pushd $d
    cargo-fmt --all -- $FMT_OPTS
    popd
done
