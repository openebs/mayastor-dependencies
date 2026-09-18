#!/usr/bin/env bash

set -euo pipefail

for d in `find -maxdepth 3 -name Cargo.toml -printf '%h\n'`; do
    pushd $d
    $@
    popd
done

