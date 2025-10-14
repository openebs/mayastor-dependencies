#!/usr/bin/env bash

helm_dep_update_required() {
  local chart="$1"

  repository=$(echo "$chart" | jq -r '.repository')
  version=$(echo "$chart" | jq -r '.version')
  name=$(echo "$chart" | jq -r '.name')
  tar=$(echo "$chart" | jq -r '.tar')

  if [ "$($SEMVER validate "$version")" != "valid" ]; then
    die "Found $name with version $version only pinned versions are supported!"
  fi

  if [ -z "$repository" ]; then
    echo "false"
    return 0
  fi

  # Special case for our floating charts
  if [[ "$($SEMVER get prerel "$version")" =~ (develop|prerelease) ]]; then
    echo "true"
    return 0
  fi

  if ! [ -f "$tar" ]; then
    echo "true"
  else
    echo "false"
  fi
}

# Returns the json for the helm chart dependencies
# In case of dependents of other local charts, an additional parent is added to the json
helm_all_deps() {
  local chart_dir="$1"
  local chart_rel="${2:-}"
  if [ -n "$chart_rel" ]; then
    chart_dir="$chart_dir/$chart_rel"
  fi
  local all_deps deps

  if ! deps=$($HELM show chart "$chart_dir" --kubeconfig "$chart_dir/fake" | $YQ -o=json ".dependencies[]" | jq -c); then
    log_fatal "Can't find the helm dependencies in $chart_dir"
  fi

  for chart in ${deps[@]}; do
    repository=$(echo "$chart" | jq -r '.repository')
    name=$(echo "$chart" | jq -r '.name')
    version=$(echo "$chart" | jq -r '.version')

    local name_rel="charts/$name"
    if [ -n "${repository:-}" ]; then
      local chart_tar="$chart_dir/$name_rel-$version.tgz"
      chart=$(echo "$chart" | jq -c ".tar = \"$chart_tar\"")

      if [ -n "$chart_rel" ]; then
        local chart_loc="$chart_dir"
        chart=$(echo "$chart" | jq -c ".chart = \"$chart_loc\"")
      fi

      if [ -n "${all_deps:-}" ]; then
        all_deps="$all_deps
        $chart"
      else
        all_deps="$chart"
      fi
      continue
    fi

    if [ -n "$chart_rel" ]; then
      name_rel="$chart_rel/$name_rel"
    fi
    # if there's no repository, it's a local chart, so check its own dependencies
    deps=$(helm_all_deps "$chart_dir" "$name_rel")
    if [ -n "${all_deps:-}" ]; then
      all_deps="$all_deps
      $deps"
    else
      all_deps="$deps"
    fi
  done

  echo "${all_deps:-}"
}

# This fetches the dependencies in an exact version from the Chart.yaml
# NOTE: Auto dependency update only works for non-pinned versions, ex: not for 14 but for 14.0.0
# NOTE: Also floating versions such as -develop and -prerelease need to be force updated
# Update can be forced with global var HELM_DEP_UPDATE="true" or cli --helm-update
helm_dep_update() {
  local chart_dir="$1"
  local update="false"
  local deps chart_loc

  deps=$(helm_all_deps "$chart_dir")

  # no dependencies?
  if [ -z "${deps:-}" ]; then
    return 0
  fi

  if [ "$HELM_DEP_UPDATE" = "true" ]; then
    update="true"
  else
    while read -r chart; do
      update_required=$(helm_dep_update_required "$chart")
      if [ "$update_required" = "true" ]; then
        update="true"
        break
      fi
    done <<< "$deps"
  fi

  if [ "$update" = "true" ]; then
    echo "Updating helm chart dependencies ..."
    $HELM dependency update "$chart_dir" --kubeconfig "$chart_dir/fake"
    while read -r chart; do
      chart_loc=$(echo "$chart" | jq -r '.chart')
      if [ -n "$chart_loc" ] && [ "$chart_loc" != "null" ]; then
        $HELM dependency update "$chart_loc" --kubeconfig "$chart_dir/fake"
      fi
    done <<< "$deps"
  fi
}
