#!/usr/bin/env bash
# Self-test for tools/check-pg-tviews-pin.sh: a version other than the pin fails, the pin
# passes, and a tree with no pin fails loudly.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
gate="$here/check-pg-tviews-pin.sh"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
fails=0
case_rc() { # name expected-rc pin doc-line
  local dir="$work/$1"; mkdir -p "$dir/docker/pg-tviews" "$dir/docs"
  [ -n "$3" ] && printf 'ARG PG_TVIEWS_TAG=%s\n' "$3" >"$dir/docker/pg-tviews/Dockerfile"
  printf '%s\n' "$4" >"$dir/docs/guide.md"
  PG_TVIEWS_PIN_ROOT="$dir" bash "$gate" >/dev/null 2>&1; local rc=$?
  if [ "$rc" -eq "$2" ]; then echo "  ok   $1"; else echo "  FAIL $1 (rc $rc, want $2)"; fails=$((fails+1)); fi
}
case_rc "a doc naming another version fails" 1 v0.1.0-beta.26 "pg_tviews v0.1.0-beta.25 is pinned"
case_rc "a doc naming it without the v fails" 1 v0.1.0-beta.26 "pg_tviews 0.1.0-beta.27"
case_rc "the pinned version passes" 0 v0.1.0-beta.26 "pg_tviews v0.1.0-beta.26 is pinned"
case_rc "no pin fails loudly" 2 "" "pg_tviews v0.1.0-beta.26"
echo "pg_tviews pin gate: 4 cases, $fails failed"
[ "$fails" -eq 0 ]
