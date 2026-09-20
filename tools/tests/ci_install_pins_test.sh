#!/usr/bin/env bash
# ci_install_pins_test.sh — the red capability of tools/check-ci-install-pins.sh.
#
# The gate exists because an unpinned `pip install` in `.dagger/main.go` let two PyPI
# releases redden an untouched branch. A gate that cannot go red on that exact line
# would be a second copy of the same false comfort, so every shape below is asserted
# in both directions: the offending spelling fails, and its pinned twin passes.
#
# Note the fixture rule: every green fixture carries a `pip install --upgrade pip`
# line, because the gate's one exemption must still match something. That is the
# stale-exemption direction, and case 8 asserts it by leaving the line out.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-ci-install-pins.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  ( cd "$1" && CI_INSTALL_PINS_ROOT="$1" bash "$gate" ) > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}
rc() { cat "${tmp}/rc"; }
said() { grep -F "$1" "${tmp}/out" >/dev/null; }

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

# Every fixture needs the exemption's trigger, or the stale check fires and the run is
# red for a reason the case under test did not intend.
exemption_line() { mk "${1}/.github/workflows/bootstrap.yml" '      - run: python -m pip install --upgrade pip'; }

# ── 1. An unpinned `pip install` fails ───────────────────────────────────────────────
root="${tmp}/pip-bare"; exemption_line "$root"
mk "${root}/.github/workflows/ci.yml" '      - run: pip install uv'
run_gate "$root"
if [ "$(rc)" = "1" ] && said "ci.yml:1 installs without naming a version"; then
  pass "an unpinned pip install fails the gate"
else
  fail "an unpinned pip install should fail the gate"
fi

# ── 2. …and its pinned twin passes ───────────────────────────────────────────────────
root="${tmp}/pip-pinned"; exemption_line "$root"
mk "${root}/.github/workflows/ci.yml" "      - run: pip install 'uv==0.12.17'"
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "a pinned pip install passes"
else
  fail "a pinned pip install should pass"
fi

# ── 3. A requirements file is a pin set ──────────────────────────────────────────────
root="${tmp}/pip-reqs"; exemption_line "$root"
mk "${root}/.dagger/main.go" '		"/tmp/v/bin/pip install --quiet -r /src/tools/scim-conformance-requirements.txt",'
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "installing from a requirements file passes"
else
  fail "installing from a requirements file should pass"
fi

# ── 4. An unpinned `cargo install` fails; --version and --path pass ──────────────────
root="${tmp}/cargo-bare"; exemption_line "$root"
mk "${root}/.github/workflows/bench.yml" '        run: cargo install critcmp --locked'
run_gate "$root"
if [ "$(rc)" = "1" ] && said "bench.yml:1 installs without naming a version"; then
  pass "an unpinned cargo install fails the gate (--locked is not a version)"
else
  fail "an unpinned cargo install should fail the gate"
fi

root="${tmp}/cargo-pinned"; exemption_line "$root"
mk "${root}/.github/workflows/bench.yml" '        run: cargo install critcmp --version 0.1.8 --locked'
mk "${root}/Makefile" '	cargo install --path crates/fraiseql-cli'
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "--version passes, and --path (a local build) is not an install from a registry"
else
  fail "--version and --path should pass"
fi

# ── 5. A fenced block is documentation, not a step ───────────────────────────────────
root="${tmp}/fenced"; exemption_line "$root"
mk "${root}/.github/workflows/sbom.yml" '        ```bash
        pip install fraiseql
        ```'
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "an install inside a fenced block is documentation, not a step"
else
  fail "a fenced install should not be reported"
fi

# ── 6. A commented-out install is not a step either ──────────────────────────────────
root="${tmp}/commented"; exemption_line "$root"
mk "${root}/tools/helper.sh" '# pip install pyarrow  (how the probe image used to do it)'
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "a commented-out install is not reported"
else
  fail "a commented-out install should not be reported"
fi

# ── 7. An empty scope is not a pass ──────────────────────────────────────────────────
root="${tmp}/empty"; mkdir -p "$root"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "no files in scope"; then
  pass "a tree the gate can see nothing in fails rather than reporting OK"
else
  fail "an empty scope should fail rather than report OK"
fi

# ── 8. A never-matched exemption is reported ─────────────────────────────────────────
root="${tmp}/stale"
mk "${root}/.github/workflows/ci.yml" "      - run: pip install 'uv==0.12.17'"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "never matched"; then
  pass "an exemption that stops triggering fails the run"
else
  fail "a stale exemption should fail the run"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} case(s) — check-ci-install-pins.sh does not discriminate as claimed" >&2
  exit 1
fi
echo "OK: check-ci-install-pins.sh goes red on each shape it names, and stays green on their twins."
