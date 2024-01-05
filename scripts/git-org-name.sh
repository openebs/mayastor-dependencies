#!/usr/bin/env bash

set -euo pipefail

die() {
  local _return="${2:-1}"
  echo "$1" >&2
  exit "${_return}"
}

# Returns the git orgname from a given git url (both https and git)
# Example: git@github.com:openebs/mayastor.git => openebs
urlOrgName() {
  local gitUrl="$1"
  if echo "$gitUrl" | grep -qa "^git@"; then
    echo "$gitUrl" | awk -F/ '/git/ { print $1 }' | cut -d':' -f2
  elif echo "$gitUrl" | grep -qa "^https://"; then
    echo "$gitUrl" | awk -F/ '/https/ { print $4 }'
  else
    die "Unknown git url type: '$gitUrl'"
  fi
}

orgName() {
  if ! git rev-parse --is-inside-work-tree &>/dev/null; then
    die "Not a git repo? '$(pwd)'"
  fi
  if ! originUrl=$(git config --get remote."$1".url); then
    die "Git url not found for remote: '$1'"
  fi
  if [ "$originUrl" = "" ]; then
    die "Remote origin url is empty!"
  fi
  echo "$originUrl"
}

toCase() {
  local org="$1"
  local case="$2"

  case "$case" in
    ""|"original")
      echo -n "$org"
      ;;
    "lower")
      echo -n "${org,,}"
      ;;
    "upper")
      echo -n "${org^^}"
      ;;
    *)
      die "Invalid case: '$case'"
      ;;
  esac
}

help() {
  cat <<EOF

Usage: $0 [OPTIONS]

Options:
  -h/--help           Display the help message and exit.
  -c/--case   <case>  Displays the org name in either "original", "lower" or "upper" case.
  -r/--remote <name>  If specified use the given git remote name, otherwise "origin" is used.

  <repo>            If set, output org name from the specified location, otherwise from current location.

Examples:
  $0 --case lower
  $0 -c upper "git/io-engine"
  $0 "git/io-engine" --remote upstream
EOF
}

REPO=
TO_CASE=
GIT_REMOTE="origin"
while [ "$#" -gt 0 ]; do
  case "$1" in
    -h|--help)
      help
      exit 0
      ;;
    -c|--case)
      test $# -lt 2 && die "Missing value for the optional argument: '$1'"
      shift
      TO_CASE="$1"
      shift
      ;;
    -r|--remote)
      test $# -lt 2 && die "Missing value for the optional argument: '$1'"
      shift
      GIT_REMOTE="$1"
      shift
      ;;
    *)
      REPO="$1"
      shift
      ;;
  esac
done

if [ -n "$REPO" ]; then
  if [ ! -d "$REPO" ]; then
    die "Given repo location is not valid: '$REPO'"
  fi
  cd "$REPO"
fi

orgName="$(orgName "$GIT_REMOTE")"
urlOrgName=$(urlOrgName "$orgName")
toCase "$urlOrgName" "$TO_CASE"

