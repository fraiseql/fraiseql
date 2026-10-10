#!/usr/bin/env bash
# check-pg-tviews-pin.sh — every pg_tviews version this repository names is the one the test
# image builds.
#
# The pin lives in one place, `docker/pg-tviews/Dockerfile` (`ARG PG_TVIEWS_TAG=`). The
# compose image tag, the docs and the suites name it too; a bump that moved the Dockerfile
# and not the docs would publish a version nothing tests (#1391). A line that mentions
# pg_tviews and a version must name the pinned one.
#
# Pure bash, no toolchain, no git history → Dagger ShellGates.
#
# Overrides, for testing:
#   PG_TVIEWS_PIN_ROOT=<dir>   scan this directory instead of the repository root
set -uo pipefail

if [ -n "${PG_TVIEWS_PIN_ROOT:-}" ]; then
  cd "$PG_TVIEWS_PIN_ROOT" || exit 1
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root" || exit 1
fi

dockerfile=docker/pg-tviews/Dockerfile
pin="$(sed -nE 's/^ARG PG_TVIEWS_TAG=(v[0-9][^[:space:]]*)$/\1/p' "$dockerfile" 2>/dev/null)"
if [ -z "$pin" ]; then
  echo "check-pg-tviews-pin: no ARG PG_TVIEWS_TAG= in $dockerfile; the gate has nothing to hold the rest to" >&2
  exit 2
fi

status=0
checked=0
while IFS= read -r hit; do
  checked=$((checked + 1))
  file="${hit%%:*}"
  rest="${hit#*:}"
  line="${rest%%:*}"
  text="${rest#*:}"
  for version in $(grep -oE 'v?0\.[0-9]+\.[0-9]+(-[a-z]+\.[0-9]+)?' <<<"$text"); do
    if [ "v${version#v}" != "$pin" ]; then
      echo "check-pg-tviews-pin: $file:$line names pg_tviews $version; the image builds $pin" >&2
      status=1
    fi
  done
done < <(grep -rnE --include='*.md' --include='*.yml' --include='*.yaml' --include='*.rs' \
           --include='Makefile' --include='Dockerfile' -i 'pg[_-]tviews.*v?0\.[0-9]+\.[0-9]+' \
           docs docker crates Makefile 2>/dev/null)

if [ "$status" -eq 0 ]; then
  echo "check-pg-tviews-pin: OK ($checked line(s) name pg_tviews $pin)"
fi
exit "$status"
