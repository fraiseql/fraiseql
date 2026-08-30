#!/usr/bin/env bash
# doc_image_refs_test.sh — the red capability of tools/check-doc-image-refs.sh.
#
# Both spellings have to be reachable. Written as one regex, the `--image=` form matched
# nothing while the gate reported OK — the shape it exists to catch, in the gate itself.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-doc-image-refs.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  ( cd "$1" && DOC_IMAGE_REFS_ROOT="$1" bash "$gate" ) > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

# ── 1. A bare `image:` fails ─────────────────────────────────────────────────────────
root="${tmp}/bare"
mk "${root}/deploy/guide.md" '    image: fraiseql:1.8.0-hardened'
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "bare FraiseQL image: fraiseql:1.8.0-hardened" "${tmp}/out" >/dev/null; then
  pass "a bare 'image:' reference fails the gate"
else
  fail "a bare 'image:' reference should fail the gate"
fi

# ── 2. A bare `--image=` fails too ───────────────────────────────────────────────────
root="${tmp}/bareflag"
mk "${root}/docs/deploy.md" '  --image=fraiseql:2.15.0 \'
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "bare FraiseQL image: fraiseql:2.15.0" "${tmp}/out" >/dev/null; then
  pass "a bare '--image=' reference fails the gate"
else
  fail "a bare '--image=' reference should fail the gate"
fi

# ── 3. Namespaced and registry-qualified references pass ─────────────────────────────
root="${tmp}/ok"
mk "${root}/deploy/guide.md" '    image: ghcr.io/fraiseql/server:2.15.0'
printf '  --image=fraiseql/server:2.15.0 \\\n' >> "${root}/deploy/guide.md"
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "0" ] && grep -F "all 2 FraiseQL image reference(s)" "${tmp}/out" >/dev/null; then
  pass "namespaced references pass, and both spellings are counted"
else
  fail "namespaced references should pass and both spellings should be counted"
fi

# ── 4. A third-party image is not this gate's business ───────────────────────────────
root="${tmp}/third"
mk "${root}/deploy/guide.md" '    image: postgres:16-alpine'
printf '    image: ghcr.io/fraiseql/server:2.15.0\n' >> "${root}/deploy/guide.md"
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "0" ] && grep -F "all 1 FraiseQL image reference(s)" "${tmp}/out" >/dev/null; then
  pass "a bare third-party image is ignored — official images are legitimately bare"
else
  fail "a bare third-party image should be ignored"
fi

# ── 5. Matching nothing is a failure, not a pass ─────────────────────────────────────
root="${tmp}/empty"
mk "${root}/docs/x.md" 'Nothing to see here.'
run_gate "$root"
if [ "$(cat "${tmp}/rc")" = "1" ] && grep -F "no FraiseQL image reference found" "${tmp}/out" >/dev/null; then
  pass "a tree with no references fails rather than reporting OK"
else
  fail "a tree with no references should fail"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} check-doc-image-refs.sh assertion(s) failed"
  exit 1
fi
echo "OK: check-doc-image-refs.sh can go red on both spellings and on an empty scan."
