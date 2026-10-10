#!/usr/bin/env bash
# check-dagger-apt.sh — every apt install in the Dagger module refreshes the index in the
# same exec, through `aptInstall`.
#
# Background (2026-10-10): `shellBase()` ran `apt-get update` as an exec of its own, cached
# as a layer, and `CheckRExamples` installed `r-base-core` in a later exec. When Ubuntu's
# security pocket replaced libpng, the cached index still named the removed .deb, and the
# install 404'd on every preflight run, rerun or not, until the cache was evicted. Any
# install exec layered on a separately cached `apt-get update` has the same defect the day
# its layer misses the cache.
#
# The rule: no exec in `.dagger/*.go` passes `apt-get` as an argument vector
# (`"apt-get", "update"` / `"apt-get", "install"`). `aptInstall` builds the one shell form.
#
# Pure bash, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   DAGGER_APT_ROOT=<dir>   scan <dir>/.dagger instead of the repository's
set -uo pipefail

if [ -n "${DAGGER_APT_ROOT:-}" ]; then
  cd "$DAGGER_APT_ROOT" || exit 1
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root" || exit 1
fi

shopt -s nullglob
files=(.dagger/*.go)
if [ "${#files[@]}" -eq 0 ]; then
  echo "check-dagger-apt: no .dagger/*.go under $(pwd); the gate sees nothing" >&2
  exit 2
fi

status=0
for f in "${files[@]}"; do
  while IFS= read -r hit; do
    echo "check-dagger-apt: $f:$hit: apt-get as an argument vector; use aptInstall(...)" >&2
    status=1
  done < <(grep -nE '"apt-get",[[:space:]]*"(update|install)"' "$f" | cut -d: -f1)
done

if [ "$status" -eq 0 ]; then
  echo "check-dagger-apt: OK (${#files[@]} files; every apt install refreshes its index)"
fi
exit "$status"
