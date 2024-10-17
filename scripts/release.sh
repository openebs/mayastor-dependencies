#!/usr/bin/env bash

# A helper script to Build and upload mayastor docker images to dockerhub repository.
# Use --dry-run to just see what would happen.
# The script assumes that a user is logged on to dockerhub for public images,
# or has insecure registry access setup for CI.

# Another script must source this script, with the given env variables:
# PROJECT: name of the project (useful for dep caching)
# CARGO_DEPS: name-path of the nix derivation containing the deps (default: $PROJECT.project-builder.cargoDeps)
# IMAGES: name of the container images

# Example usage:
# #!/usr/bin/env bash
#
## Build and upload mayastor docker images to dockerhub repository.
## Use --dry-run to just see what would happen.
## The script assumes that a user is logged on to dockerhub for public images,
## or has insecure registry access setup for CI.
#
#IMAGES="mayastor.io-engine mayastor.casperf fio-spdk"
#CARGO_DEPS=units.cargoDeps
#PROJECT="io-engine" . ./scripts/release_.sh
#
#common_run $@

# Write output to error output stream.
echo_stderr() {
  echo -e "${1}" >&2
}

# Write out error and exit process with specified error or 1.
die()
{
  local _return="${2:-1}"
  echo_stderr "$1"
  exit "${_return}"
}

set -euo pipefail

# Test if the image already exists in the image registry
dockerhub_tag_exists() {
  if ! $CURL --silent -f -lSL https://hub.docker.com/v2/repositories/"${1#docker.io/}"/tags/"$2" 1>/dev/null 2>&1; then
    # If the registry has a port specified, then maybe it's a local registry
    # so let's try it in the following format
    if [[ "$REGISTRY" =~ ':' ]]; then
      $CURL --silent -f -lSL http://"$REGISTRY"/v2/"${1#$REGISTRY}"/manifests/"$2" 1>/dev/null 2>&1
    else
      return 1
    fi
  fi
}

# Get the tag at the HEAD
get_tag() {
  vers=$(git describe --exact-match 2>/dev/null || echo "")
  echo -n "$vers"
}
get_hash() {
  vers=$(git rev-parse --short=12 HEAD)
  echo -n "$vers"
}
nix_experimental() {
  if (nix eval 2>&1 || true) | grep "extra-experimental-features" 1>/dev/null; then
      echo -n " --extra-experimental-features nix-command "
  else
      echo -n " "
  fi
}
pre_fetch_cargo_deps() {
  local nixAttrPath=$1
  local project=$2
  local maxAttempt=$3

  local outLink="--no-out-link"
  local cargoVendorMsg=""
  if [ -n "$CARGO_VENDOR_DIR" ]; then
    if [ "$(realpath -ms "$CARGO_VENDOR_DIR")" = "$(realpath -ms "$SCRIPT_DIR/..")" ]; then
      cargoVendorDir="$CARGO_VENDOR_DIR/$GIT_BRANCH"
    else
      cargoVendorDir="$CARGO_VENDOR_DIR/$project/$GIT_BRANCH"
    fi
    cargoVendorMsg="into $(realpath -ms "$cargoVendorDir") "
    outLink="--out-link $cargoVendorDir"
  fi

  for (( attempt=1; attempt<=maxAttempt; attempt++ )); do
     if $NIX_BUILD $outLink -A "$nixAttrPath"; then
       echo "Cargo vendored dependencies pre-fetched ""$cargoVendorMsg""after $attempt attempt(s)"
       return 0
     fi
     sleep 1
  done
  if [ "$attempt" = "1" ]; then
    echo "Cargo vendor pre-fetch is disabled"
    return 0
  fi

  die "Failed to pre-fetch the cargo vendored dependencies in $maxAttempt attempts"
}
# Setup DOCKER with the docker or podman (which is mostly cli compat with docker and thus
# we can simply use it as an alias) cli.
# If present, the env variable DOCKER is checked for the binary, with precedence.
docker_alias() {
  DOCKER_CLIS=("docker" "podman")
  if [ -n "${DOCKER:-}" ]; then
    DOCKER_CLIS=("$DOCKER" ${DOCKER_CLIS[@]})
  fi
  for cli in "${DOCKER_CLIS[@]}"; do
    if binary_check "$cli" "info"; then
      echo "$cli"
      return
    fi
  done
  binary_missing_die "docker compatible"
}

# Check if the binaries are present, otherwise bail out.
binaries_check() {
  FAIL=
  for bin in $@; do
    if ! binary_check "$bin"; then
      binary_missing_msg "$bin"
      FAIL="y"
    fi
  done
  if [ -n "$FAIL" ]; then
    exit 1
  fi
}
# Check if the binary name is present, otherwise error out.
binary_check() {
  check=${2:-"--version"}
  if ! "$1" "$check" &>/dev/null; then
    return 1
  fi
}
# Check if the binary name is present, otherwise die out.
binary_check_die() {
  if ! binary_check $@; then
    binary_missing_die "$1"
  fi
}
# Bail out with binary missing (arg 1) error
binary_missing_die() {
  die "$(binary_missing_msg "$1")"
}
# Get the binary missing error message
binary_missing_msg() {
  echo "$1 binary missing - please install it and add it to your PATH"
}

# Parse all the common arguments
# This only works if there is no additional argument added by the parent script.
# Otherwise add a new copy of this function with the additional arguments inline.
parse_common_args() {
  while [ "$#" -gt 0 ]; do
    case $1 in
      -h|--help)
        help_
        exit 0
        ;;
      *)
        parse_common_arg $@
        set -- $ARGS
        ;;
    esac
  done
}

# Validates that argument does not start with "-"
validate_arg() {
  if [[ "${2:--}" =~ ^-.* ]]; then
    die "Missing $1 argument"
  fi
}

help_() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

$(common_help)

Examples:
  $(basename "$0") --registry 127.0.0.1:5000
EOF
}

parse_common_arg() {
  case $1 in
    -d|--dry-run)
      DOCKER="echo $DOCKER"
      NIX_BUILD="echo $NIX_BUILD"
      NIX_SHELL="echo $NIX_SHELL"
      RM="echo $RM"
      TAR="echo $TAR"
      HELM="echo $HELM"
      shift
      ;;
    --registry)
      shift
      REGISTRY=$1
      shift
      ;;
    --alias-tag)
      shift
      ALIAS=$1
      shift
      ;;
    --tag)
      shift
      if [ "$TAG" != "" ] && [ "$TAG" != "$1" ]; then
        echo "Overriding $TAG with $1"
      fi
      TAG=$1
      shift
      ;;
    --image)
      shift
      validate_arg "Image name" "${1:-}"
      IMAGES="${IMAGES:-} $1"
      shift
      ;;
    --skip-images)
      shift
      DEFAULT_IMAGES=
      IMAGES=
      ;;
    --tar)
      shift
      IMAGE_LOAD_TAR="yes"
      ;;
    --skip-build)
      SKIP_BUILD="yes"
      shift
      ;;
    --skip-publish)
      SKIP_PUBLISH="yes"
      shift
      ;;
    --debug)
      BUILD_TYPE="debug"
      shift
      ;;
    --incremental)
      INCREMENTAL="true"
      shift
      ;;
    --build-bin)
      shift
      validate_arg "Binary name" "${1:-}"
      BUILD_BINARIES="${BUILD_BINARIES:-} $1"
      shift
      ;;
    --build-bins)
      BUILD_BINARIES="$DEFAULT_BINARIES"
      shift
      ;;
    --no-static-linking)
      STATIC_LINKING="false"
      shift
      ;;
    --skip-bins)
      shift
      BUILD_BINARIES=
      DEFAULT_BINARIES=
      ;;
    --build-binary-out)
      shift
      BINARY_OUT_LINK="$1"
      shift
      ;;
    --skopeo-copy)
      CONTAINER_LOAD=
      shift
      ;;
    --skip-cargo-deps)
      SKIP_CARGO_DEPS="yes"
      shift
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
  ARGS=$@
}

# Setup various build variables
setup() {
  if [ -n "$SKIP_BUILD" ] && [ -z "$CONTAINER_LOAD" ]; then
    die "--skopeo-copy currently incompatible with --skip-build"
  fi

  if [ -z "$IMAGES" ]; then
    IMAGES="$DEFAULT_IMAGES"
  elif [ "$(echo "$IMAGES" | wc -w)" == "1" ]; then
    image=$(echo "$IMAGES" | xargs)
    if $NIX_EVAL -f . "images.debug.$image.imageName" 1>/dev/null 2>/dev/null; then
      if [ "$INCREMENTAL" == "true" ]; then
        # if we're building a single image incrementally, then build only that image
        ALL_IN_ONE="false"
      fi
    fi
  fi
  if [ -z "$BUILD_BINARIES" ]; then
    BUILD_BINARIES="$DEFAULT_BINARIES"
  fi

  # Create alias
  ALIAS_TAG=
  if [ -n "$ALIAS" ]; then
    ALIAS_TAG=$ALIAS
    # when alias is created from branch-name we want to keep the hash and have it pushed to CI because
    # the alias will change daily.
    OVERRIDE_COMMIT_HASH="true"
  elif [ "$BRANCH" == "develop" ]; then
    ALIAS_TAG="$BRANCH"
  elif [ "${BRANCH#release-}" != "${BRANCH}" ]; then
    ALIAS_TAG="${BRANCH}"
  fi

  if [ -n "$TAG" ] && [ "$TAG" != "$(get_tag)" ]; then
    # Set the TAG which basically allows building the binaries as if it were a git tag
    NIX_TAG_ARGS="--argstr tag $TAG"
    NIX_BUILD="$NIX_BUILD $NIX_TAG_ARGS"
    ALIAS_TAG=
  fi
  TAG=${TAG:-$HASH}
  if [ -n "$OVERRIDE_COMMIT_HASH" ] && [ -n "$ALIAS_TAG" ]; then
    # Set the TAG to the alias and remove the alias
    NIX_TAG_ARGS="--argstr img_tag $ALIAS_TAG"
    NIX_BUILD="$NIX_BUILD $NIX_TAG_ARGS"
    TAG="$ALIAS_TAG"
    ALIAS_TAG=
  fi
}

cache_deps() {
  if [ -n "$SKIP_CARGO_DEPS" ]; then
    return
  fi
  if [ -z "${PROJECT:-}" ]; then
    die "Caching cargo deps requires \$PROJECT env to be set"
  fi
  ## pre-fetch build dependencies with a number of attempts to harden against flaky networks
  pre_fetch_cargo_deps "$CARGO_DEPS" "mayastor-$PROJECT" "$CARGO_VENDOR_ATTEMPTS"
}

# Execute skopeo commands, either using local binary (if present) or nix-shell
exec_skopeo() {
  if [ -n "$LOCAL_SKOPEO" ]; then
    $SKOPEO $@
  else
    $NIX_SHELL -p "(import (import $NIX_SOURCES).nixpkgs {}).skopeo" --run "$SKOPEO $@"
  fi
}

skopeo_check() {
  # Currently skopeo is only used for this
  if [ -n "$CONTAINER_LOAD" ]; then
    return
  fi

  if binary_check "$SKOPEO"; then
    LOCAL_SKOPEO="y"
  else
    if [ ! -f "$NIX_SOURCES" ]; then
      die "$(binary_missing_msg "$SKOPEO") or set NIX_SOURCES so we can pull it from nixpkgs"
    fi
    NIX_SOURCES=$(realpath "$NIX_SOURCES")
    binary_check_die "$NIX_SHELL"
    if ! exec_skopeo "--version" &>/dev/null; then
      binary_check_die "$SKOPEO"
    fi
  fi
}

helm_check() {
  # Currently helm is only used for this
  if [ -z "$HELM_DEPS_IMAGES" ]; then
    return
  fi
  # todo: need to check for specific version?
  if binary_check "$HELM" "version"; then
    LOCAL_HELM="y"
  else
    if [ ! -f "$NIX_SOURCES" ]; then
      die "$(binary_missing_msg "$HELM") or set NIX_SOURCES so we can pull it from nixpkgs"
    fi
    NIX_SOURCES=$(realpath "$NIX_SOURCES")
    binary_check_die "$NIX_SHELL"
    if ! exec_helm "version" &>/dev/null; then
      binary_check_die "$HELM"
    fi
  fi
  binary_check_die "$TAR"
}

# Execute helm commands, either using local binary (if present) or nix-shell
exec_helm() {
  if [ -n "$LOCAL_HELM" ]; then
    $HELM $@
  else
    $NIX_SHELL -p "(import (import $NIX_SOURCES).nixpkgs {}).kubernetes-helm-wrapped" --run "$HELM $@"
  fi
}

build_helm_deps() {
  local build_deps=
  for helm_image in $HELM_DEPS_IMAGES; do
    for image in $IMAGES; do
      if [ "$image" = "$helm_image" ]; then
        build_deps="y"
        break 2
      fi
    done
  done

  if [ -z "$build_deps" ]; then
    return
  fi

  echo "Updating helm chart dependencies ..."
  # Helm chart directory path -- /scripts --> /chart
  chart_dir="$SCRIPT_DIR/../chart"
  dep_chart_dir="$chart_dir/charts"

  # This performs a dependency update and then extracts the tarballs pulled.
  # If and when the `--untar` functionality is added to the `helm dependency
  # update command, the for block can be removed in favour of the `--untar` option.
  # Ref: https://github.com/helm/helm/issues/8479
  exec_helm "dependency update $chart_dir"
  for dep_chart_tar in "$dep_chart_dir"/*.tgz; do
    $TAR -xf "$dep_chart_tar" -C "$dep_chart_dir"
    $RM -f "$dep_chart_tar"
  done
}

build_images() {
  for name in $IMAGES; do
    image_basename=$($NIX_EVAL -f . "images.$BUILD_TYPE.$name.imageName" --raw --quiet --argstr product_prefix "$PRODUCT_PREFIX")
    image=$image_basename
    archive=$name

    if [ -n "$REGISTRY" ]; then
      if [[ "$REGISTRY" =~ '/' ]]; then
        image="$REGISTRY/$(echo "$image" | cut -d'/' -f2)"
      else
        image="$REGISTRY/$image"
      fi
    fi

    UPLOAD_NAMES+=("$image")
    UPLOAD_TARS+=("$(realpath -s "$archive-image")")

    # If we're skipping the build, then we just want to upload
    # the images we already have locally.
    if [ -z "$SKIP_BUILD" ]; then
      echo "Building $image:$TAG ..."
      $NIX_BUILD --out-link "$archive-image" -A "images.$BUILD_TYPE.$archive" --arg allInOne "$ALL_IN_ONE" --arg incremental "$INCREMENTAL" --argstr product_prefix "$PRODUCT_PREFIX"
      if [ -n "$CONTAINER_LOAD" ]; then
        container_load "$archive-image"
        if [ "$image" != "$image_basename" ]; then
          echo "Renaming $image_basename:$TAG to $image:$TAG"
          $DOCKER tag "${image_basename}:$TAG" "$image:$TAG"
          $DOCKER image rm "${image_basename}:$TAG"
        fi
      fi
    fi
  done
}

# Load the container image into the host service.
container_load() {
  if [ -n "$IMAGE_LOAD_TAR" ]; then
    container_load_tar "$1"
  else
    if ! $DOCKER load -i "$1"; then
      if $DOCKER "version" | grep -i "podman" &>/dev/null; then
        IMAGE_LOAD_TAR="yes"
        echo_stderr "Failed to load compressed docker image on podman, trying uncompressed image..."
        container_load_tar "$1"
      else
        return 1
      fi
    fi
  fi

  $RM "$1"
}
# Load the container image into the host service.
container_load_tar() {
  $ZCAT "$1" > "$1.tar"
  $DOCKER load -i "$1.tar"
  $RM "$1.tar"
}

upload_image_alias() {
  img=$1
  tag=$2
  alias=$3
  tar=$4

  if [ -n "$CONTAINER_LOAD" ]; then
    $DOCKER tag "$img:$tag" "$img:$alias"
  fi
  upload_image "$img" "$alias" "$tar"
}
upload_image() {
  img=$1
  tag=$2
  tar=$3

  if [ -n "$CONTAINER_LOAD" ]; then
    echo "Uploading $img:$tag to registry ..."
    $DOCKER push "$img:$tag"
  elif [ -n "$tar" ]; then
    echo "Uploading $img:$tag to registry ..."
    exec_skopeo copy docker-archive:"$tar" docker://"$img:$tag"
  else
    die "Missing tar file... can't upload image"
  fi
}

upload_images() {
  # sanity check both arrays, just in case...
  if [ "${#UPLOAD_NAMES[*]}" != "${#UPLOAD_TARS[*]}" ]; then
    die "Upload image names array doesn't match the image tar archives"
  fi

  if (( ${#UPLOAD_NAMES[*]} )) && [ -z "$SKIP_PUBLISH" ]; then
    for i in "${!UPLOAD_NAMES[@]}"; do
      img="${UPLOAD_NAMES[$i]}"
      tar="${UPLOAD_TARS[$i]}"

      # Should this be an override instead?
      if [ -n "$CI" ] && dockerhub_tag_exists "$img" "$TAG"; then
        echo "Skipping $img:$TAG which already exists"
        continue
      fi

      upload_image "$img" "$TAG" "$tar"
      if [ -n "$ALIAS_TAG" ]; then
        upload_image_alias "$img" "$TAG" "$ALIAS_TAG" "$tar"
      fi
    done
  fi

  $DOCKER image prune -f
}

cleanup_tars() {
  for tar in "${UPLOAD_TARS[@]}"; do
    $RM -f "$tar"
  done
}

build_bins() {
  if [ -n "$BUILD_BINARIES" ]; then
    mkdir -p "$BINARY_OUT_LINK"
    for name in $BUILD_BINARIES; do
      echo "Building static $name ..."
      $NIX_BUILD --out-link "$BINARY_OUT_LINK/$name" -A "$PROJECT.$BUILD_TYPE.$name" --arg allInOne "$ALL_IN_ONE" --arg static "$STATIC_LINKING"
    done
  fi
}

# Set up the container aliases, build the binaries, and build/upload the images
common_run() {
  parse_common_args $@
  skopeo_check
  setup

  cache_deps

  build_helm_deps
  build_bins

  build_images
  upload_images
}

common_help() {
  cat <<EOF
  -d, --dry-run              Output actions that would be taken, but don't run them.
  -h, --help                 Display this text.
  --registry <host[:port]>   Push the built images to the provided registry.
                             To also replace the image org provide the full repository path, example: docker.io/org
  --debug                    Build debug version of images where possible.
  --skip-build               Don't perform nix-build.
  --skip-publish             Don't publish built images.
  --image           <image>  Specify what image to build and/or upload.
  --tar                      Decompress and load images as tar rather than tar.gz.
  --skip-images              Don't build nor upload any images.
  --alias-tag       <tag>    Explicit alias for short commit hash tag.
  --tag             <tag>    Explicit tag (overrides the git tag).
  --incremental              Builds components in two stages allowing for faster rebuilds during development.
  --build-bins               Builds all the static binaries.
  --no-static-linking        Don't build the binaries with static linking.
  --build-bin                Specify which binary to build.
  --skip-bins                Don't build the static binaries.
  --build-binary-out <path>  Specify the outlink path for the binaries (otherwise it's the current directory).
  --skopeo-copy              Don't load containers into host, simply copy them to registry with skopeo.
  --skip-cargo-deps          Don't prefetch the cargo build dependencies.
EOF
}

CI=${CI-}
DOCKER=$(docker_alias)
NIX_BUILD="nix-build"
NIX_EVAL="nix eval$(nix_experimental)"
NIX_SHELL="nix-shell"
RM="rm"
TAR="tar"
HELM="helm"
CURL="curl"
SKOPEO="skopeo"
ZCAT="zcat"
SCRIPT_DIR=$(dirname "$0")
TAG=$(get_tag)
HASH=$(get_hash)
PRODUCT_PREFIX=${MAYASTOR_PRODUCT_PREFIX:-""}
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
BRANCH=${GIT_BRANCH////-}
UPLOAD_NAMES=()
UPLOAD_TARS=()
SKIP_PUBLISH=
SKIP_BUILD=
OVERRIDE_COMMIT_HASH=
REGISTRY=
ALIAS=
BUILD_TYPE="release"
ALL_IN_ONE="true"
INCREMENTAL="false"
DEFAULT_BINARIES=${BUILD_BINARIES:-}
BUILD_BINARIES=
STATIC_LINKING="true"
BINARY_OUT_LINK="."
CARGO_VENDOR_DIR=${CARGO_VENDOR_DIR:-}
CARGO_VENDOR_ATTEMPTS=${CARGO_VENDOR_ATTEMPTS:-25}
CARGO_DEPS=${CARGO_DEPS:-${PROJECT:-}.project-builder.cargoDeps}
SKIP_CARGO_DEPS=
DEFAULT_IMAGES=$IMAGES
IMAGES=
IMAGE_LOAD_TAR=
# Images which require helm chart dependency update
HELM_DEPS_IMAGES=${HELM_DEPS_IMAGES:-}
LOCAL_HELM=
NIX_SOURCES=$(realpath "${NIX_SOURCES:-"$SCRIPT_DIR/../nix/sources.nix"}")
DEFAULT_COMMON_BINS=("$CURL" "$DOCKER" "$TAR" "$RM" "$NIX_BUILD" "$ZCAT")
COMMON_BINS=${COMMON_BINS:-"${DEFAULT_COMMON_BINS[@]}"}
CONTAINER_LOAD="yes"
LOCAL_SKOPEO=

binaries_check "${COMMON_BINS[@]}"
helm_check

cd "$SCRIPT_DIR/.."

trap cleanup_tars EXIT
