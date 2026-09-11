#!/usr/bin/env bash
# check-compiled-schema-stamp.sh — fail when a compiled schema that CI *boots* names a
# fraiseql build that is not the one this tree builds.
#
# Background (#1304): a compiled schema is a build artifact of the release that produced
# it, and since 2.15.0 the server refuses any artifact its own build did not produce. Two
# checked-in artifacts under docker/e2e/ are booted by `release-smoke.yml` and
# `load-test.yml`. release-smoke triggers only on `release/*` branches and `v*` tags, so a
# stale stamp is invisible on `dev` and on every feature branch and surfaces as a failed
# boot at the tag — the worst place to find it.
#
# The stamp goes stale on exactly one event: the version bump in `tools/release.sh`, which
# bumps these files for the same reason it bumps the Dockerfile label and the Helm chart
# (#1129). This gate is what fails if that call is ever removed.
#
# Discovery is by *reference*, not by presence: the subjects are the `FRAISEQL_SCHEMA_PATH`
# values in the workflows. Two reasons, both measured:
#
#   * `.gitignore` carries `*.compiled.json`, so a developer who has run any example has
#     untracked artifacts (examples/saas/, examples/multitenant/) that a `find` sweep picks
#     up. That gate would be red on developer machines and green in CI — the worst
#     direction, because it teaches people to ignore it.
#   * `git ls-files` cannot filter them out: the Dagger ShellGates container runs
#     `git init -q .`, so the index is empty and every path reads as untracked. A
#     git-based sweep passes vacuously over the whole tree there.
#
# A workflow reference exists in both shapes and names only files the repository owns.
#
# Pure grep/sed, no toolchain → Dagger ShellGates.
#
# Overrides, for testing:
#   COMPILED_STAMP_ROOT=<dir>   treat this directory as the repo root
set -euo pipefail

if [ -n "${COMPILED_STAMP_ROOT:-}" ]; then
  cd "$COMPILED_STAMP_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

if [ ! -f Cargo.toml ]; then
  echo "ERROR: no Cargo.toml at $(pwd) — cannot read the version this tree builds." >&2
  exit 2
fi

version="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)"
if [ -z "$version" ]; then
  echo "ERROR: could not read the workspace version from Cargo.toml." >&2
  exit 2
fi

workflow_dir=".github/workflows"
if [ ! -d "$workflow_dir" ]; then
  echo "ERROR: $workflow_dir not found — cannot discover what CI boots." >&2
  exit 2
fi

# Every FRAISEQL_SCHEMA_PATH a workflow sets, deduplicated. A value that is not a file in
# this tree (a container path, a variable) is skipped: the subject is what the repository
# ships, not what an operator mounts.
mapfile -t referenced < <(
  grep -rhoE '^[[:space:]]*FRAISEQL_SCHEMA_PATH:[[:space:]]*[^[:space:]]+' "$workflow_dir" \
    | sed 's/.*FRAISEQL_SCHEMA_PATH:[[:space:]]*//' \
    | sort -u
)

if [ "${#referenced[@]}" -eq 0 ]; then
  echo "ERROR: no FRAISEQL_SCHEMA_PATH found in $workflow_dir." >&2
  echo "       Either the extraction is broken or the workflows stopped booting a" >&2
  echo "       compiled schema; a gate that checks nothing must not report success." >&2
  exit 2
fi

checked=0
failed=0
for path in "${referenced[@]}"; do
  [ -f "$path" ] || continue          # container paths and mounts are not ours to check
  checked=$((checked + 1))
  stamp="$(sed -n 's/.*"fraiseql_version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$path" | head -1)"
  if [ -z "$stamp" ]; then
    echo "FAIL: $path carries no \"fraiseql_version\", and the server refuses an unstamped" >&2
    echo "      artifact — a workflow booting this file will not start it. Add" >&2
    echo "      \"fraiseql_version\": \"$version\", or recompile it with fraiseql-cli." >&2
    failed=1
  elif [ "$stamp" != "$version" ]; then
    echo "FAIL: $path names fraiseql $stamp, but this tree builds $version." >&2
    echo "      A server built here refuses it, so the workflow booting it fails." >&2
    failed=1
  fi
done

if [ "$checked" -eq 0 ]; then
  echo "ERROR: ${#referenced[@]} FRAISEQL_SCHEMA_PATH value(s) found, none of them a file" >&2
  echo "       in this tree. The gate checked nothing and must not report success." >&2
  exit 2
fi

[ "$failed" -eq 0 ] || exit 1

echo "OK: all $checked CI-booted compiled schema(s) name v${version}."
