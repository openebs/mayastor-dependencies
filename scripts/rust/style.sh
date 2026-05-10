#!/usr/bin/env bash

set -euo pipefail

FMT_OPTS=${FMT_OPTS:-"--config imports_granularity=Crate"}

cargo-fmt --all --check -- $FMT_OPTS
cargo-fmt --all -- $FMT_OPTS
