#!/usr/bin/env bash
set -euo pipefail

version=$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)

# Lefthook's `run` jobs don't receive Git's pre-push stdin. Instead, check any
# release tag pointing at the commit being pushed. This is the usual release
# workflow (tag the release commit, then push it).
while IFS= read -r tag; do
  [[ "$tag" == v* ]] || continue
  if [[ "$tag" != "v$version" ]]; then
    printf 'Refusing push: release tag %s points at HEAD, but Cargo.toml version is %s.\n' "$tag" "$version" >&2
    exit 1
  fi
done < <(git tag --points-at HEAD)
