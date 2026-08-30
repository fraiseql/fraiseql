#!/usr/bin/env bash
# compose_references_test.sh — the red capability of tools/check-compose-references.sh.
#
# Three things have to be true and none is visible from a passing run of the real tree:
# a dead reference fails, a live one passes, and a tree where the grep matches nothing
# fails rather than reporting OK over every entry point in the repository (#1219).
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-compose-references.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  ( cd "$1" && COMPOSE_REFS_ROOT="$1" bash "$gate" ) > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

# ── 1. A named compose file that does not exist is a failure ─────────────────────────
root="${tmp}/dead"; mkdir -p "$root"
printf 'up:\n\t@docker compose -f docker/gone/docker-compose.yml up -d\n' > "${root}/Makefile"
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "does not exist: docker/gone/docker-compose.yml" "${tmp}/out" >/dev/null; then
  pass "a Makefile naming a missing compose file fails the gate"
else
  fail "a Makefile naming a missing compose file should fail the gate"
fi

# ── 2. The same reference, with the file present, passes ─────────────────────────────
root="${tmp}/live"; mkdir -p "${root}/docker/here"
printf 'up:\n\t@docker compose -f docker/here/docker-compose.yml up -d\n' > "${root}/Makefile"
: > "${root}/docker/here/docker-compose.yml"
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "0" ] && grep -F "OK: all 1 Compose file reference(s) resolve" "${tmp}/out" >/dev/null; then
  pass "a reference that resolves passes the gate"
else
  fail "a reference that resolves should pass the gate"
fi

# ── 3. A comment quoting a dead path is prose, not an entry point ────────────────────
# `check-examples-integrity.sh` explains #1052 by quoting the deleted `make demo-start`
# command; this gate's own header quotes the form it greps for. Neither is runnable.
root="${tmp}/comment"; mkdir -p "${root}/docker"
printf '# historical: docker compose -f docker/docker-compose.demo.yml up\nup:\n\t@docker compose -f docker/docker-compose.test.yml up -d\n' > "${root}/Makefile"
: > "${root}/docker/docker-compose.test.yml"
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "0" ]; then
  pass "a compose path quoted in a comment is not treated as a reference"
else
  fail "a compose path in a comment should not fail the gate"
fi

# ── 4. Matching nothing is a failure, not a pass ─────────────────────────────────────
root="${tmp}/empty"; mkdir -p "$root"
printf 'build:\n\t@cargo build\n' > "${root}/Makefile"
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "no 'docker compose -f' references found" "${tmp}/out" >/dev/null; then
  pass "a tree with no references fails rather than reporting OK"
else
  fail "a tree with no references should fail"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} check-compose-references.sh assertion(s) failed"
  exit 1
fi
echo "OK: check-compose-references.sh can go red on a dead reference and on an empty scan."
