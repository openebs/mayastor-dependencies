#!/usr/bin/env bash

# Mirror container images from source registry to target registry using crane.
# Preserves multi-platform support and image digests.

set -euo pipefail

# --- Determine paths based on CHART_REPO ---
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]:-"$0"}")")"
source "${SCRIPT_DIR}/../utils/log.sh"

# --- Default Variables ---
CHART_REPO="false"
SOURCE=""
TARGET=""
TAG=""

# --- Parse arguments --- 
for ((i=1; i <= $#; i++)); do
  case "${!i}" in
    --chart)
      next=$((i+1))
      if [[ $next -le $# ]]; then
        CHART_REPO="${!next}"
      fi
      ;;
  esac
done

while [[ $# -gt 0 ]]; do
  case $1 in
    --source)
      SOURCE="$2"
      shift 2
      ;;
    --target)
      TARGET="$2"
      shift 2
      ;;
    --tag)
      TAG="$2"
      shift 2
      ;;
    --chart)
      CHART_REPO="$2"; 
      shift 2 
      ;;
    *)
      log_fatal "Unknown option: $1"
      ;;
  esac
done

if [[ -z "$SOURCE" ]] || [[ -z "$TARGET" ]] || [[ -z "$TAG" ]]; then
  log_fatal "Usage: $0 --source <source> --target <target> --tag <tag> [--chart true|false]"
fi

# --- Determine paths based on CHART_REPO ---
if [[ "${CHART_REPO}" == "true" ]]; then
    ROOT_DIR="${SCRIPT_DIR}/../../../../../../"
    : "${PARENT_ROOT_DIR:=$ROOT_DIR}"
    export SOURCE_REL="$PARENT_ROOT_DIR/dependencies/control-plane/utils/dependencies/scripts/release.sh"
else
    ROOT_DIR="${SCRIPT_DIR}/../../../../"
    : "${PARENT_ROOT_DIR:=$ROOT_DIR}"
    export SOURCE_REL="$PARENT_ROOT_DIR/utils/dependencies/scripts/release.sh"
fi

# Store the tag as it gets overriden by release.sh
input_tag="$TAG"

NO_RUN=true . "$PARENT_ROOT_DIR/scripts/release.sh"

# Restore it post sourcing release.sh
TAG="$input_tag"

IMAGES=()
for name in $DEFAULT_IMAGES; do
  image=$($NIX_EVAL -f "$PARENT_ROOT_DIR" "images.$BUILD_TYPE.$name.imageName" --raw --quiet --argstr product_prefix "$PRODUCT_PREFIX")
  IMAGES+=("${image##*/}")
done

echo "Mirroring images from ${SOURCE} to ${TARGET} with tag ${TAG}"

for IMAGE in "${IMAGES[@]}"; do
  echo "Mirroring ${IMAGE}:${TAG}..."

  SRC="${SOURCE}/${IMAGE}:${TAG}"
  DEST="${TARGET}/${IMAGE}:${TAG}"
  crane copy --platform all "${SRC}" "${DEST}"

  echo "Successfully mirrored ${IMAGE}:${TAG}"
done
