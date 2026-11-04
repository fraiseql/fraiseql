#!/usr/bin/env bash
# Self-test for tools/check-pg-tviews-pin.sh: a version other than the pin fails, the pin
# passes, a tree with no pin fails loudly, and a compose tag that does not name the pinned
# commit fails.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
gate="$here/check-pg-tviews-pin.sh"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
fails=0
commit=84157ed337b5b94baa60f3734c04f26a17a132b6
case_rc() { # name expected-rc pin doc-line [compose-image]
  local dir="$work/$1"; mkdir -p "$dir/docker/pg-tviews" "$dir/docs"
  [ -n "$3" ] && printf 'ARG PG_TVIEWS_TAG=%s\nARG PG_TVIEWS_COMMIT=%s\n' "$3" "$commit" \
    >"$dir/docker/pg-tviews/Dockerfile"
  printf '%s\n' "$4" >"$dir/docs/guide.md"
  printf 'services:\n  postgres-tviews-test:\n    image: %s\n' \
    "${5:-fraiseql/pg-tviews-test:pg18-$3-${commit:0:7}}" >"$dir/docker/docker-compose.test.yml"
  PG_TVIEWS_PIN_ROOT="$dir" bash "$gate" >/dev/null 2>&1; local rc=$?
  if [ "$rc" -eq "$2" ]; then echo "  ok   $1"; else echo "  FAIL $1 (rc $rc, want $2)"; fails=$((fails+1)); fi
}
case_rc "a doc naming another version fails" 1 v0.1.0-beta.26 "pg_tviews v0.1.0-beta.25 is pinned"
case_rc "a doc naming it without the v fails" 1 v0.1.0-beta.26 "pg_tviews 0.1.0-beta.27"
case_rc "the pinned version passes" 0 v0.1.0-beta.26 "pg_tviews v0.1.0-beta.26 is pinned"
case_rc "no pin fails loudly" 2 "" "pg_tviews v0.1.0-beta.26"
case_rc "a compose tag without the commit fails" 1 v0.1.0-beta.26 "pg_tviews v0.1.0-beta.26" \
  "fraiseql/pg-tviews-test:pg18-v0.1.0-beta.26"
case_rc "a compose tag naming another commit fails" 1 v0.1.0-beta.26 "pg_tviews v0.1.0-beta.26" \
  "fraiseql/pg-tviews-test:pg18-v0.1.0-beta.26-0d4c08a"
echo "pg_tviews pin gate: 6 cases, $fails failed"
[ "$fails" -eq 0 ]
