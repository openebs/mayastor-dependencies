#!/usr/bin/env bash

set -euo pipefail

FMT_ERROR=
FMT_OPTS=${FMT_OPTS:-"--config imports_granularity=Crate"}

cargo-fmt --all --check -- $FMT_OPTS || FMT_ERROR=$?

if [ -n "$FMT_ERROR" ]; then
  cargo-fmt --all -- $FMT_OPTS
fi

exit ${FMT_ERROR:-0}

