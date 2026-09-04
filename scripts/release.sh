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

set -euo pipefail

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]:-"$0"}")")"

source "$SCRIPT_DIR/utils/log.sh"
source "$SCRIPT_DIR/utils/helm.sh"

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
# Get the container image architecture name for the host
image_arch() {
  case "$(uname -m)" in
    x86_64)
      echo -n "amd64"
      ;;
    aarch64|arm64)
      echo -n "arm64"
      ;;
    *)
      log_fatal "Unsupported host architecture: $(uname -m)"
      ;;
  esac
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
    if [ "$(realpath -ms "$CARGO_VENDOR_DIR")" = "${PARENT_ROOT:-}" ]; then
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

  log_fatal "Failed to pre-fetch the cargo vendored dependencies in $maxAttempt attempts"
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
  log_fatal "$(binary_missing_msg "$1")"
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
    log_fatal "Missing $1 argument"
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
      PODMAN="echo $PODMAN"
      DRY_RUN="yes"
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
    --image-out)
      shift
      validate_arg "Image output directory" "${1:-}"
      # Keep the built image archives in "<dir>/<arch>/" instead of loading
      # and publishing them, to be pushed later through --manifest-tars.
      IMAGE_OUT_DIR=$1
      SKIP_PUBLISH="yes"
      CONTAINER_LOAD=
      shift
      ;;
    --manifest-tars)
      shift
      validate_arg "Manifest tars directory" "${1:-}"
      MANIFEST_TARS=$1
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
    --debug-slim)
      BUILD_TYPE="debug"
      if [ -z "${RUSTFLAGS:-}" ]; then
        RUSTFLAGS="-C debuginfo=0 -C strip=debuginfo"
      fi
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
    --helm-update)
      HELM_DEP_UPDATE="true"
      shift
      ;;
    --sbom)
      SBOM="yes"
      shift
      ;;
    --sbom-out)
      shift
      validate_arg "SBOM output directory" "${1:-}"
      SBOM_OUT="$1"
      shift
      ;;
    --insecure-registry)
      INSECURE_REGISTRY="yes"
      shift
      ;;
    --cosign-key)
      shift
      validate_arg "Cosign key" "${1:-}"
      COSIGN_KEY="$1"
      shift
      ;;
    --tlog-upload)
      TLOG_UPLOAD="true"
      shift
      ;;
    --no-tlog-upload)
      TLOG_UPLOAD="false"
      shift
      ;;
    --force-attest)
      FORCE_ATTEST="yes"
      ATTEST="yes"
      SBOM="yes"
      shift
      ;;
    --attest)
      ATTEST="yes"
      # There's nothing to attest without an SBOM to attest to.
      SBOM="yes"
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
    log_fatal "--skopeo-copy currently incompatible with --skip-build"
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
    log_fatal "Caching cargo deps requires \$PROJECT env to be set"
  fi
  ## pre-fetch build dependencies with a number of attempts to harden against flaky networks
  pre_fetch_cargo_deps "$CARGO_DEPS" "mayastor-$PROJECT" "$CARGO_VENDOR_ATTEMPTS"
}

skopeo_check() {
  # skopeo copies the images to the registry, and resolves the digests of what
  # we've pushed so that the signatures and attestations can be bound to them.
  if [ -z "$ATTEST" ] && { [ -n "$CONTAINER_LOAD" ] || [ -n "$IMAGE_OUT_DIR" ]; }; then
    return
  fi
  if ! binary_check "$SKOPEO"; then
    SKOPEO=$(fetch_nix_bin "skopeo" "skopeo")
  fi
}

podman_check() {
  # Currently podman is only used to assemble and push the multi-arch manifests
  if [ -z "$MANIFEST_TARS" ] || [ -n "$DRY_RUN" ]; then
    return
  fi
  if ! binary_check "$PODMAN"; then
    PODMAN=$(fetch_nix_bin "podman" "podman")
  fi
}

sbom_check() {
  if [ -z "$SBOM" ]; then
    return
  fi
  if ! binary_check "$SYFT"; then
    SYFT=$(fetch_nix_bin "syft" "syft")
  fi
}

cosign_check() {
  if [ -z "$ATTEST" ]; then
    return
  fi
  if ! binary_check "$COSIGN" "version"; then
    COSIGN=$(fetch_nix_bin "cosign" "cosign")
  fi
  # Used to pick the per-arch manifests out of a pushed manifest list, and to
  # look for artifacts already attached to an image.
  if ! binary_check "$JQ"; then
    JQ=$(fetch_nix_bin "jq" "jq")
  fi
  if ! binary_check "$ORAS" "version"; then
    ORAS=$(fetch_nix_bin "oras" "oras")
  fi
  # Signing a local registry is a test, and keyless signing is almost certainly
  # not what's wanted for one: it authenticates interactively through a browser
  # and puts the identity it authenticated as in a permanent public record.
  if [ -n "$INSECURE_REGISTRY" ] && [ -z "$COSIGN_KEY" ]; then
    log_warn "Signing keyless: cosign will ask to authenticate through a browser."
    log_warn "Pass --cosign-key to sign with a key pair instead."
  fi
}

helm_check() {
  # Currently helm is only used for this
  if [ -z "$HELM_DEPS_IMAGES" ]; then
    return
  fi
  # todo: need to check for specific version?
  if ! binary_check "$HELM" "version"; then
    HELM=$(fetch_nix_bin "kubernetes-helm-wrapped" "helm")
  fi
  binary_check_die "$TAR"
  if ! binary_check "$SEMVER"; then
    SEMVER=$(fetch_nix_bin "semver-tool" "semver")
  fi
  if ! binary_check "$JQ"; then
    JQ=$(fetch_nix_bin "jq" "jq")
  fi
  if ! binary_check "$YQ"; then
    YQ=$(fetch_nix_bin "yq-go" "yq")
  fi
}

fetch_nix_bin() {
  local package="$1"
  local bin="$2"

  if [ ! -f "$NIX_SOURCES" ]; then
    log_fatal "$(binary_missing_msg "$package") or set NIX_SOURCES so we can pull it from nixpkgs"
  fi
  NIX_SOURCES=$(realpath "$NIX_SOURCES")
  binary_check_die "$NIX"

  $NIX shell --impure $(nix_experimental) --expr "(import (import $NIX_SOURCES).nixpkgs {}).$package" -c bash -c "type -P $bin"
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
  if [ -z "$HELM_CHART_DIR" ]; then
    log_fatal "Some of the images require `helm dependency update` on the helm chart, but the chart directory is not set"
  fi

  helm_dep_update "${HELM_CHART_DIR%/}"
}

# Prefix the image name with the registry, if set
registry_image_name() {
  local image=$1

  if [ -n "$REGISTRY" ]; then
    if [[ "$REGISTRY" =~ '/' ]]; then
      image="$REGISTRY/$(echo "$image" | cut -d'/' -f2)"
    else
      image="$REGISTRY/$image"
    fi
  fi

  echo -n "$image"
}

# Generate the SBOM for the given image, from the image itself rather than from
# its nix closure: that picks up dependencies vendored into a single derivation
# (rust crates, say), which a closure scan cannot see, and it describes what is
# actually in the layers rather than what the image was built from.
#
# With --image-out the SBOM is written next to the image archive, under
# "<dir>/<arch>/", so that each architecture is scanned natively on its own
# runner and the manifest push can later attest each arch with its own SBOM.
build_image_sbom() {
  local name=$1
  local ref=$2
  local archive=$3
  local out="$(sbom_dir)/$name"

  mkdir -p "$(sbom_dir)"
  echo "Generating SBOM for $name ..."

  if [ -n "$DRY_RUN" ]; then
    echo "$SYFT scan <$name image> -o cyclonedx-json=$out.cdx.json"
    return
  fi

  if [ -n "$CONTAINER_LOAD" ]; then
    # Just loaded into the container service, so scan it there rather than
    # unpacking the archive again.
    $SYFT scan "docker:$ref:$TAG" -o cyclonedx-json="$out.cdx.json" -q
    return
  fi

  # syft reads a docker archive, but not the compressed one nix produces.
  $ZCAT "$archive" > "$archive.tar"
  $SYFT scan "docker-archive:$archive.tar" -o cyclonedx-json="$out.cdx.json" -q
  $RM -f "$archive.tar"
}

# Where the SBOMs are written: alongside the per-arch image archives when those
# are being kept, so they travel together to the manifest push.
sbom_dir() {
  if [ -n "$IMAGE_OUT_DIR" ]; then
    echo -n "$IMAGE_OUT_DIR/$(image_arch)"
  else
    echo -n "$SBOM_OUT"
  fi
}

# Resolve the digest of the manifest at the given reference. We sign and attest
# digests rather than tags, since a tag is mutable. For a manifest list this is
# the digest of the list itself.
image_digest() {
  $SKOPEO inspect ${INSECURE_REGISTRY:+--tls-verify=false} --no-tags --format '{{.Digest}}' "docker://$1"
}

# List "<arch> <digest>" for each linux image within the manifest list at the
# given reference.
manifest_children() {
  $SKOPEO inspect ${INSECURE_REGISTRY:+--tls-verify=false} --raw "docker://$1" \
    | $JQ -r '.manifests[]? | select(.platform.os == "linux" and .platform.architecture != "unknown") | "\(.platform.architecture) \(.digest)"'
}

# Whether the signing events should go to the public transparency log.
#
# Uploaded by default, as cosign does: for keyless signing the Rekor entry is
# what proves the signature was made while the short-lived certificate was still
# valid, so without it "cosign verify" needs --insecure-ignore-tlog. The default
# is flipped for --insecure-registry, since signing a local registry for a test
# has no business in a permanent public log. --tlog-upload/--no-tlog-upload
# override either way.
tlog_upload() {
  if [ -n "$TLOG_UPLOAD" ]; then
    echo -n "$TLOG_UPLOAD"
  elif [ -n "$INSECURE_REGISTRY" ]; then
    echo -n "false"
  else
    echo -n "true"
  fi
}

# Add the arguments which attach the attestation as a sigstore bundle, to the
# named array.
#
# cosign 2 needs --new-bundle-format for that; cosign 3 does it by default, has
# deprecated the flag and dropped it from its help, so it is probed for rather
# than passed unconditionally - both to avoid the deprecation warning and
# because the flag will eventually be removed.
bundle_args() {
  local -n out=$1

  if [ -z "$COSIGN_BUNDLE_FLAG" ]; then
    if $COSIGN attest --help 2>/dev/null | grep -q -- "--new-bundle-format"; then
      COSIGN_BUNDLE_FLAG="yes"
    else
      COSIGN_BUNDLE_FLAG="no"
    fi
  fi
  if [ "$COSIGN_BUNDLE_FLAG" = "yes" ]; then
    out+=(--new-bundle-format)
  fi
}

# Add the arguments which stop a signing event from being recorded in the
# transparency log, to the named array.
#
# cosign 3 deprecated --tlog-upload in favour of a signing config listing the
# services to use, so there it is given one with no transparency log. The config
# is created from scratch rather than fetched, so this stays offline. cosign 2
# has no such thing and takes the flag. Probed for rather than matched on the
# version.
no_tlog_args() {
  local -n out=$1

  if [ -z "$COSIGN_SIGNING_CONFIG" ]; then
    if $COSIGN sign --help 2>/dev/null | grep -q -- "--signing-config="; then
      if [ -n "$DRY_RUN" ]; then
        COSIGN_SIGNING_CONFIG="<no-tlog signing config>"
      else
        COSIGN_SIGNING_CONFIG=$(mktemp -t cosign-no-tlog-XXXXXX.json)
        $COSIGN signing-config create --out "$COSIGN_SIGNING_CONFIG" 1>/dev/null
      fi
    else
      COSIGN_SIGNING_CONFIG="none"
    fi
  fi
  if [ "$COSIGN_SIGNING_CONFIG" = "none" ]; then
    out+=(--tlog-upload=false)
  else
    out+=(--signing-config "$COSIGN_SIGNING_CONFIG")
  fi
}

# Whether anything is already attached to the given digest reference, ie whether
# it has been signed and attested before.
#
# cosign attaches both as referrers and cannot replace them: --replace only ever
# applied to the older tag layout - it is a no-op for these, measurably - and
# cosign 3 dropped the flag. So signing an unchanged digest again just leaves
# duplicates behind, and a re-run of an already published tag skips instead.
already_signed() {
  if [ -n "$FORCE_ATTEST" ]; then
    return 1
  fi
  $ORAS discover ${INSECURE_REGISTRY:+--plain-http} --format json "$1" 2>/dev/null \
    | $JQ -e '(.manifests // []) | length > 0' 1>/dev/null 2>&1
}

# Sign the given digest reference. Any extra arguments go to cosign sign.
#
# The signature is attached as an OCI 1.1 referrer rather than as a
# "sha256-<digest>.sig" tag. Note that verifying it then needs
# "cosign verify --experimental-oci11"; without that cosign looks for the legacy
# tag and reports "no signatures found", which looks just like an unsigned image.
sign_ref() {
  local ref=$1
  shift
  local args=(--registry-referrers-mode oci-1-1 "$@")

  if [ -n "$INSECURE_REGISTRY" ]; then
    args+=(--allow-insecure-registry)
  fi
  if [ "$(tlog_upload)" = "false" ]; then
    no_tlog_args args
  fi
  if [ -n "$COSIGN_KEY" ]; then
    args+=(--key "$COSIGN_KEY")
  fi

  if [ -n "$DRY_RUN" ]; then
    echo "COSIGN_EXPERIMENTAL=1 $COSIGN sign --yes ${args[*]} $ref"
    return
  fi

  echo "Signing $ref ..."
  # COSIGN_EXPERIMENTAL is what gates the oci-1-1 referrers mode, as of cosign
  # 3.1.3; "attest --new-bundle-format" below does not need it.
  COSIGN_EXPERIMENTAL=1 $COSIGN sign --yes "${args[@]}" "$ref"
}

# Attach the given SBOM to the reference as an in-toto attestation, as a sigstore
# bundle - which lands as an OCI 1.1 referrer, and is verified with
# "cosign verify-attestation --new-bundle-format" (NOT --experimental-oci11,
# which is parsed but never reaches attestation verification, see
# sigstore/cosign#4708).
attest_ref() {
  local ref=$1
  local sbom=$2
  local args=()

  bundle_args args

  if [ -n "$INSECURE_REGISTRY" ]; then
    args+=(--allow-insecure-registry)
  fi
  if [ "$(tlog_upload)" = "false" ]; then
    no_tlog_args args
  fi
  if [ -n "$COSIGN_KEY" ]; then
    args+=(--key "$COSIGN_KEY")
  fi

  if [ -n "$DRY_RUN" ]; then
    echo "$COSIGN attest --yes ${args[*]} --type cyclonedx --predicate $sbom $ref"
    return
  fi

  if [ ! -f "$sbom" ]; then
    log_fatal "Missing SBOM $sbom, can't attest $ref"
  fi
  echo "Attesting the SBOM of $ref ..."
  $COSIGN attest --yes "${args[@]}" --type cyclonedx --predicate "$sbom" "$ref"
}

# Sign a single-architecture image and attach its SBOM.
attest_image() {
  local img=$1
  local tag=$2
  local sbom=$3

  if [ -n "$DRY_RUN" ]; then
    sign_ref "$img:$tag"
    attest_ref "$img:$tag" "$sbom"
    return
  fi

  local digest
  digest=$(image_digest "$img:$tag") || log_fatal "Failed to resolve the digest of $img:$tag"
  if [ -z "$digest" ]; then
    log_fatal "Empty digest for $img:$tag"
  fi

  if already_signed "$img@$digest"; then
    echo "Skipping $img@$digest which is already signed"
    return
  fi

  sign_ref "$img@$digest"
  attest_ref "$img@$digest" "$sbom"
}

# Sign a pushed manifest list and attach the per-arch SBOMs.
#
# The list is signed with --recursive so that each architecture's manifest is
# signed too and can be verified on its own digest. The SBOMs, however, are
# attached per architecture rather than to the list: each one describes only the
# arch it was scanned on, so attesting the list with any single one of them would
# claim contents that were never scanned.
attest_manifest() {
  local img=$1
  local tag=$2
  local name=$3

  if [ -n "$DRY_RUN" ]; then
    sign_ref "$img:$tag" -r
    attest_ref "$img@<per-arch digest>" "<per-arch sbom>"
    return
  fi

  local digest
  digest=$(image_digest "$img:$tag") || log_fatal "Failed to resolve the digest of $img:$tag"
  if [ -z "$digest" ]; then
    log_fatal "Empty digest for $img:$tag"
  fi

  if already_signed "$img@$digest"; then
    echo "Skipping $img@$digest which is already signed"
    return
  fi

  sign_ref "$img@$digest" -r

  local arch child found=
  while read -r arch child; do
    if [ -z "$arch" ]; then
      continue
    fi
    found="y"
    attest_ref "$img@$child" "$MANIFEST_TARS/$arch/$name.cdx.json"
  done < <(manifest_children "$img:$tag")

  if [ -z "$found" ]; then
    log_fatal "No per-arch manifests found in $img:$tag, nothing to attest"
  fi
}

build_images() {
  local out_dir="."
  if [ -n "$IMAGE_OUT_DIR" ]; then
    out_dir="$IMAGE_OUT_DIR/$(image_arch)"
    mkdir -p "$out_dir"
  fi

  for name in $IMAGES; do
    image_basename=$($NIX_EVAL -f . "images.$BUILD_TYPE.$name.imageName" --raw --quiet --argstr product_prefix "$PRODUCT_PREFIX")
    image=$(registry_image_name "$image_basename")
    archive=$name

    UPLOAD_NAMES+=("$image")
    UPLOAD_TARS+=("$(realpath -s "$out_dir/$archive-image")")
    UPLOAD_SBOMS+=("$(sbom_dir)/$archive.cdx.json")

    # If we're skipping the build, then we just want to upload
    # the images we already have locally.
    if [ -z "$SKIP_BUILD" ]; then
      echo "Building $image:$TAG ..."
      $NIX_BUILD --out-link "$out_dir/$archive-image" -A "images.$BUILD_TYPE.$archive" --arg allInOne "$ALL_IN_ONE" --arg incremental "$INCREMENTAL" --argstr product_prefix "$PRODUCT_PREFIX" --argstr rustFlags "$RUSTFLAGS"
      if [ -n "$CONTAINER_LOAD" ]; then
        container_load "$out_dir/$archive-image"
        if [ "$image" != "$image_basename" ]; then
          echo "Renaming $image_basename:$TAG to $image:$TAG"
          $DOCKER tag "${image_basename}:$TAG" "$image:$TAG"
          $DOCKER image rm "${image_basename}:$TAG"
        fi
      fi
    fi

    # After the image rather than before: it's scanned off the built image, which
    # by now is either loaded into the container service or sitting in $out_dir.
    if [ -n "$SBOM" ]; then
      build_image_sbom "$archive" "$image" "$(realpath -s "$out_dir/$archive-image")"
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
        log_warn "Failed to load compressed docker image on podman, trying uncompressed image..."
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
    $SKOPEO copy docker-archive:"$tar" docker://"$img:$tag"
  else
    log_fatal "Missing tar file... can't upload image"
  fi
}

upload_images() {
  # sanity check the arrays, just in case...
  if [ "${#UPLOAD_NAMES[*]}" != "${#UPLOAD_TARS[*]}" ] || [ "${#UPLOAD_NAMES[*]}" != "${#UPLOAD_SBOMS[*]}" ]; then
    log_fatal "Upload image names array doesn't match the image tar archives"
  fi

  if (( ${#UPLOAD_NAMES[*]} )) && [ -z "$SKIP_PUBLISH" ]; then
    for i in "${!UPLOAD_NAMES[@]}"; do
      img="${UPLOAD_NAMES[$i]}"
      tar="${UPLOAD_TARS[$i]}"

      # Should this be an override instead?
      if [[ -n "$CI" && ! "$TAG" =~ -(develop|prerelease)$ ]] && dockerhub_tag_exists "$img" "$TAG"; then
        echo "Skipping the upload of $img:$TAG which already exists"
      else
        upload_image "$img" "$TAG" "$tar"
        if [ -n "$ALIAS_TAG" ]; then
          upload_image_alias "$img" "$TAG" "$ALIAS_TAG" "$tar"
        fi
      fi

      # Deliberately outside the upload: an image already in the registry still
      # needs its signature and SBOM, and skipping the push is no reason to leave
      # it unsigned. The alias points at the same manifest and cosign attaches to
      # the digest, so attesting once covers every tag of this image.
      if [ -n "$ATTEST" ]; then
        attest_image "$img" "$TAG" "${UPLOAD_SBOMS[$i]}"
      fi
    done
  fi

  $DOCKER image prune -f
}

cleanup() {
  # A real temp file even under --dry-run's echo'd $RM, so remove it directly.
  if [ -f "$COSIGN_SIGNING_CONFIG" ]; then
    rm -f "$COSIGN_SIGNING_CONFIG"
  fi
  cleanup_tars
}

cleanup_tars() {
  # The image archives are kept for a later --manifest-tars push
  if [ -n "$IMAGE_OUT_DIR" ]; then
    return 0
  fi
  for tar in "${UPLOAD_TARS[@]}"; do
    $RM -f "$tar"
  done
}

# Assemble and push multi-arch manifest lists (tagged $TAG and $ALIAS_TAG) from the per-arch
# image archives found in "$MANIFEST_TARS/<arch>/<name>-image", as produced by --image-out.
# The arch images are pushed untagged (addressed by digest) along with the manifest list.
create_manifests() {
  if [ -n "$SKIP_PUBLISH" ]; then
    echo "Skipping the manifest creation ..."
    return 0
  fi

  for name in $IMAGES; do
    image_basename=$($NIX_EVAL -f . "images.$BUILD_TYPE.$name.imageName" --raw --quiet --argstr product_prefix "$PRODUCT_PREFIX")
    image=$(registry_image_name "$image_basename")
    list="localhost/$name-manifest"

    $PODMAN manifest rm "$list" &>/dev/null || true
    $PODMAN manifest create "$list"

    # podman reads the archives lazily, only when the manifest is pushed, so the
    # decompressed tars must be kept around until after the push.
    tars=()
    for tar in "$MANIFEST_TARS"/*/"$name-image"; do
      if [ ! -f "$tar" ]; then
        log_fatal "No image archives found for image $name in $MANIFEST_TARS"
      fi
      # podman can't read compressed docker archives
      if [ -n "$DRY_RUN" ]; then
        echo "$ZCAT $tar > $tar.tar"
      else
        $ZCAT "$tar" > "$tar.tar"
      fi
      tars+=("$tar.tar")
      echo "Adding $tar to the $image manifest ..."
      $PODMAN manifest add "$list" docker-archive:"$tar.tar"
    done

    for tag in $TAG $ALIAS_TAG; do
      echo "Pushing manifest $image:$tag ..."
      $PODMAN manifest push --all "$list" docker://"$image:$tag"
    done

    # Once only: the alias list has the same digest, and cosign attaches to
    # digests rather than tags.
    if [ -n "$ATTEST" ]; then
      attest_manifest "$image" "$TAG" "$name"
    fi

    $PODMAN manifest rm "$list"
    $RM "${tars[@]}"
  done
}

build_bins() {
  if [ -n "$BUILD_BINARIES" ]; then
    mkdir -p "$BINARY_OUT_LINK"
    for name in $BUILD_BINARIES; do
      echo "Building static $name ..."
      $NIX_BUILD --out-link "$BINARY_OUT_LINK/$name" -A "$PROJECT.$BUILD_TYPE.$name" --arg allInOne "$ALL_IN_ONE" --arg static "$STATIC_LINKING" --argstr rustFlags "$RUSTFLAGS"
    done
  fi
}

get_nix_src() {
  if [ -n "${PARENT_ROOT}" ]; then
    echo "$(realpath "${NIX_SOURCES:-${PARENT_ROOT}/nix/sources.nix}")"
  else
    echo "$(realpath "${NIX_SOURCES}")"
  fi
}

get_parent() {
  # Check if specified by parent
  if [ -n "${PARENT_ROOT_DIR:-}" ]; then
    echo "$PARENT_ROOT_DIR"
    return 0
  fi

  # Mayastor control-plane
  if [ -n "${WORKSPACE_ROOT:-}" ]; then
    echo "$WORKSPACE_ROOT"
    return 0
  fi

  # Mayastor extensions
  if [ -n "${EXTENSIONS_SRC:-}" ]; then
    echo "$EXTENSIONS_SRC"
    return 0
  fi

  # OpenEBS umbrella
  if [ -n "${OPENEBS_SRC:-}" ]; then
    echo "$OPENEBS_SRC"
    return 0
  fi

  # Figure it out using the git top-level
  if command -v git &>/dev/null; then
    git rev-parse --show-toplevel
    return 0
  fi

  # From the current path
  if [ -d "${PWD:-.}/nix" ]; then
    echo "${PWD:-.}"
    return 0
  fi

  # Using the io-engine
  if [ -n "${SRCDIR:-}" ]; then
    echo "$SRCDIR"
    return 0
  fi
}

# Set up the container aliases, build the binaries, and build/upload the images
common_run() {
  parse_common_args $@
  skopeo_check
  podman_check
  sbom_check
  cosign_check
  setup

  if [ -n "$MANIFEST_TARS" ]; then
    create_manifests
    return 0
  fi

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
  --debug-slim               Build slim debug version of images where possible. (sets RUSTFLAGS="-C debuginfo=0 -C strip=debuginfo")
  --skip-build               Don't perform nix-build.
  --skip-publish             Don't publish built images.
  --image           <image>  Specify what image to build and/or upload.
  --tar                      Decompress and load images as tar rather than tar.gz.
  --skip-images              Don't build nor upload any images.
  --alias-tag       <tag>    Explicit alias for short commit hash tag.
  --tag             <tag>    Explicit tag (overrides the git tag).
  --image-out       <dir>    Keep the built image archives in "<dir>/<arch>/" instead of
                             loading and publishing them (implies --skip-publish), to be
                             pushed later through --manifest-tars.
  --manifest-tars   <dir>    Assemble and push multi-arch manifest lists from the per-arch image
                             archives kept by previous --image-out runs (skips building).
                             The arch images are pushed untagged, addressed only by digest.
  --incremental              Builds components in two stages allowing for faster rebuilds during development.
  --build-bins               Builds all the static binaries.
  --no-static-linking        Don't build the binaries with static linking.
  --build-bin                Specify which binary to build.
  --skip-bins                Don't build the static binaries.
  --build-binary-out <path>  Specify the outlink path for the binaries (otherwise it's the current directory).
  --skopeo-copy              Don't load containers into host, simply copy them to registry with skopeo.
  --skip-cargo-deps          Don't prefetch the cargo build dependencies.
  --helm-update              Force update helm dependencies.
  --sbom                     Generate a CycloneDX SBOM per image, with syft.
  --sbom-out        <path>   Directory to write the SBOMs to (default: ./sbom).
                             Ignored with --image-out, which keeps them beside
                             the per-arch image archives.
  --attest                   Sign the pushed images and attach their SBOM as an
                             in-toto attestation. Implies --sbom.
  --force-attest             Sign and attest even if the image is signed already,
                             leaving what is already attached in place. Implies
                             --attest.
  --insecure-registry        Talk to the registry over plain HTTP, for testing
                             against a local registry. Also stops the signatures
                             from being recorded in the public transparency log.
  --cosign-key      <path>   Sign with this cosign key rather than keyless, which
                             avoids cosign authenticating through a browser.
                             Defaults to \$COSIGN_KEY.
  --tlog-upload              Record the signatures in the public transparency log
                             even when --insecure-registry is given.
  --no-tlog-upload           Never record the signatures in the public
                             transparency log. Note that verifying a keyless
                             signature then needs --insecure-ignore-tlog.

Environment Variables:
  RUSTFLAGS                  Set Rust compiler options when building binaries.
  COSIGN_KEY                 Default for --cosign-key.
EOF
}

CI=${CI-}
DOCKER=$(docker_alias)
NIX_BUILD="nix-build"
NIX="nix"
NIX_EVAL="$NIX eval$(nix_experimental)"
NIX_SHELL="nix-shell"
RM="rm"
TAR="tar"
HELM="helm"
CURL="curl"
SKOPEO="skopeo"
ORAS="oras"
SYFT="syft"
COSIGN="cosign"
ZCAT="zcat"
SEMVER="semver"
YQ="yq"
JQ="jq"
SCRIPT_DIR=$(dirname "$0")
PARENT_ROOT="$(realpath -es "$(get_parent)" 2>/dev/null || :)"
TAG=$(get_tag)
HASH=$(get_hash)
PRODUCT_PREFIX=${MAYASTOR_PRODUCT_PREFIX:-""}
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
BRANCH=${GIT_BRANCH////-}
UPLOAD_NAMES=()
UPLOAD_TARS=()
UPLOAD_SBOMS=()
SKIP_PUBLISH=
SBOM=
SBOM_OUT="sbom"
ATTEST=
INSECURE_REGISTRY=
FORCE_ATTEST=
TLOG_UPLOAD=
COSIGN_KEY=${COSIGN_KEY:-}
COSIGN_SIGNING_CONFIG=
COSIGN_BUNDLE_FLAG=
SKIP_BUILD=
OVERRIDE_COMMIT_HASH=
REGISTRY=
ALIAS=
IMAGE_OUT_DIR=
MANIFEST_TARS=
PODMAN="podman"
DRY_RUN=
BUILD_TYPE="release"
ALL_IN_ONE="true"
INCREMENTAL="false"
DEFAULT_BINARIES=${BUILD_BINARIES:-}
BUILD_BINARIES=
STATIC_LINKING="true"
RUSTFLAGS=${RUSTFLAGS:-}
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
HELM_CHART_DIR=${HELM_CHART_DIR:-}
NIX_SOURCES=$(get_nix_src)
DEFAULT_COMMON_BINS=("$CURL" "$DOCKER" "$TAR" "$RM" "$NIX_BUILD" "$ZCAT" "$NIX")
COMMON_BINS=${COMMON_BINS:-"${DEFAULT_COMMON_BINS[@]}"}
CONTAINER_LOAD="yes"
LOCAL_SKOPEO=
# Force helm dependency update
HELM_DEP_UPDATE=${HELM_DEP_UPDATE:-"false"}

binaries_check "${COMMON_BINS[@]}"
helm_check

cd "$PARENT_ROOT"

trap cleanup EXIT
