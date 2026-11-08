#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]:-"$0"}")")"
source "$SCRIPT_DIR/utils/log.sh"

MODULES=()
HUGEPAGES=

help() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -h, --help              Display this text.
  --nvme-tcp              Load the nvme_tcp kernel module.
  --rdma                  Load the nvme_rdma and rdma_rxe (soft-RoCE) kernel modules.
  --module <name>         Load the given kernel module (may be repeated).
  --hugepages <num>       Ensure at least <num> 2MiB hugepages are allocated.

Kernel modules which can't be loaded are installed via the distro package manager, if supported.

Examples:
  $(basename "$0") --nvme-tcp --hugepages 2048
EOF
}

setup_hugepages() {
  local wanted="$1"
  local actual
  actual=$(sysctl -n vm.nr_hugepages)
  if [ "$actual" -ge "$wanted" ]; then
    log "Current hugepages ($actual) are sufficient"
    return
  fi
  sysctl -w vm.nr_hugepages="$wanted"
  actual=$(sysctl -n vm.nr_hugepages)
  if [ "$actual" != "$wanted" ]; then
    log_fatal "Wanted $wanted hugepages, but only allocated $actual"
  fi
}

install_kernel_modules() {
  . /etc/os-release
  case "${ID:-}" in
    ubuntu)
      log "Installing linux-modules-extra-$(uname -r) ..."
      # Skip the slow man-db index rebuild triggered by package installs
      debconf-communicate <<< "set man-db/auto-update false" || true
      dpkg-reconfigure man-db || true
      apt-get update || (sleep 5; apt-get update)
      apt-get install -y "linux-modules-extra-$(uname -r)"
      ;;
    *)
      log_fatal "Don't know how to install kernel modules on ${NAME:-unknown}"
      ;;
  esac
}

load_modules() {
  local missing=()
  local module
  for module in "$@"; do
    if [ -d "/sys/module/$module" ]; then
      log "$module is already loaded"
    elif modprobe "$module" 2>/dev/null; then
      log "Loaded $module"
    else
      missing+=("$module")
    fi
  done

  if [ ${#missing[@]} -eq 0 ]; then
    return
  fi

  log "Kernel modules not available: ${missing[*]}"
  install_kernel_modules
  for module in "${missing[@]}"; do
    modprobe "$module" || log_fatal "Failed to load $module"
    log "Loaded $module"
  done
}

while [ "$#" -gt 0 ]; do
  case $1 in
    -h|--help)
      help
      exit 0
      ;;
    --nvme-tcp)
      MODULES+=(nvme_tcp)
      ;;
    --rdma)
      MODULES+=(nvme_rdma rdma_rxe)
      ;;
    --module)
      shift
      [ $# -ge 1 ] || log_fatal "Missing module name"
      MODULES+=("$1")
      ;;
    --hugepages)
      shift
      [ $# -ge 1 ] || log_fatal "Missing hugepage number"
      [[ "$1" =~ ^[0-9]+$ ]] || log_fatal "Invalid hugepage number: $1"
      HUGEPAGES=$1
      ;;
    *)
      help
      log_fatal "Unknown argument $1"
      ;;
  esac
  shift
done

if [ -n "$HUGEPAGES" ]; then
  setup_hugepages "$HUGEPAGES"
fi

if [ ${#MODULES[@]} -gt 0 ]; then
  load_modules "${MODULES[@]}"
fi
