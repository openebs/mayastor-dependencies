#!/usr/bin/env bash

set -euo pipefail

cargo-clippy --all --all-targets -- -D warnings
