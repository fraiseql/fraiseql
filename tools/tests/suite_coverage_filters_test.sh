#!/usr/bin/env bash
# Red-capability pin for the FILTER-LIVENESS side of tools/check-suite-coverage.py (#1300).
#
# Run directly:  bash tools/tests/suite_coverage_filters_test.sh
# Exits non-zero if any assertion fails.
#
# A positional filter narrows a cargo run to the tests whose full name CONTAINS
# it. A filter that matches nothing is not an error: cargo prints `running 0
# tests` / `test result: ok. 0 passed` and exits 0, so the leg stays green and the
# suite the line was added to run has silently stopped running. Thirty invocations
# in `.dagger/main.go` carry one — several naming a single test function, the
# longest 63 characters of path — and nothing checked that any still named
# something that exists.
#
# The coverage scan cannot answer it. `covers_module` reads a filter matching no
# discovered module as "this invocation does not cover that module", which is the
# right conservative answer for coverage and is exactly why a BROKEN filter is
# indistinguishable there from one that was never meant to match.
#
# No Rust toolchain, no cargo, no network: the gate reads source as text.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-suite-coverage.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <dagger cargo line>
#
# One crate carrying every shape a filter can name: a plain lib test, a test
# inside an inline `mod`, a module reached through `#[path = "…"]`, and an
# integration binary with a test of its own. The invocation under test is the
# only thing that varies, so every verdict is attributable to it.
make_fixture() {
    local dir="$1" invocation="$2"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/crates/demo/src" "$dir/crates/demo/tests" \
             "$dir/.github/workflows"
    cp "$GATE" "$dir/tools/check-suite-coverage.py"

    cat >"$dir/.github/workflows/probe.yml" <<'YML'
name: Probe
on:
  push:
jobs:
  gates:
    runs-on: ubuntu-latest
    steps:
      - run: dagger call test --source=.
YML

    cat >"$dir/tools/required-checks.toml" <<'TOML'
required = ["gates"]
TOML

    cat >"$dir/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/demo"]
TOML

    cat >"$dir/crates/demo/Cargo.toml" <<'TOML'
[package]
name = "demo"
version = "0.0.0"
edition = "2021"
TOML

    cat >"$dir/crates/demo/src/lib.rs" <<'RS'
pub fn thing() -> u8 { 1 }

pub mod inbound;

#[cfg(test)]
mod tests {
    #[test]
    fn a_plain_lib_test() {}
}
RS

    cat >"$dir/crates/demo/src/inbound.rs" <<'RS'
#[cfg(test)]
#[path = "inbound_tests.rs"]
mod tests;
RS

    cat >"$dir/crates/demo/src/inbound_tests.rs" <<'RS'
#[test]
fn reached_only_through_a_path_attribute() {}
RS

    cat >"$dir/crates/demo/tests/probe.rs" <<'RS'
#[test]
fn a_binary_test() {}
RS

    cat >"$dir/crates/demo/tests/other.rs" <<'RS'
#[test]
fn a_test_in_the_other_binary() {}
RS

    cat >"$dir/.dagger/main.go" <<GO
package main

func (m *FraiseqlCi) Test() string {
	script := []string{
		"cargo test -p demo --test probe --test other",
		"$invocation",
	}
	return script[0]
}
GO
}

# expect <label> <expected-exit> <fixture-dir> [<substring-that-must-appear>]
expect() {
    local label="$1" want="$2" dir="$3" needle="${4:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local out rc
    set +e
    out="$(cd "$dir" && python3 tools/check-suite-coverage.py 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want" ]; then
        echo "FAIL  $label: exit $rc, wanted $want"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle"; then
        echo "FAIL  $label: output did not mention '$needle'"
        printf '%s\n' "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "PASS  $label"
}

echo "suite-coverage filter liveness"
echo

echo "── a filter that names something, and one that names nothing ──"

make_fixture "$WORK/live" "cargo test -p demo --lib -- tests::"
expect "a filter naming a live module is accepted" 0 "$WORK/live"

make_fixture "$WORK/dead" "cargo test -p demo --lib -- tsets::"
expect "a filter naming nothing is a DEAD FILTER" 1 "$WORK/dead" \
    "DEAD FILTER Test: \`tsets::\`"

echo
echo "── the filter set is test functions, not only modules ──"
#
# Ten of the thirty sites name a single test fn — the longest is
# `server_config::tests::resolve_storage_section_parses_bucket_policies`. Matching
# against discovered MODULES alone would report every one of them dead.

make_fixture "$WORK/fn" "cargo test -p demo --lib -- tests::a_plain_lib_test"
expect "a filter naming a test function is accepted" 0 "$WORK/fn"

make_fixture "$WORK/fn_typo" "cargo test -p demo --lib -- tests::a_plain_lib_tset"
expect "...and a typo in that function name is not" 1 "$WORK/fn_typo" \
    "DEAD FILTER Test: \`tests::a_plain_lib_tset\`"

echo
echo "── a module reached through #[path] is still reachable ──"
#
# Twenty modules in this tree are declared `#[path = "x_tests.rs"] mod tests;`.
# Resolving `mod tests;` the ordinary way finds nothing there, and a scan that
# gave up would report every filter naming one of them as dead — a false failure
# on a live leg line, which is the direction that costs a real gate.

make_fixture "$WORK/pathattr" "cargo test -p demo --lib -- inbound::tests::reached_only_through_a_path_attribute"
expect "a filter naming a #[path]-declared module's test is accepted" 0 "$WORK/pathattr"

echo
echo "── --test restricts the pool to the binaries it names ──"

make_fixture "$WORK/bin" "cargo test -p demo --test probe -- a_binary_test"
expect "a filter matching a test in the named binary is accepted" 0 "$WORK/bin"

make_fixture "$WORK/bin_wrong" "cargo test -p demo --test probe -- a_test_in_the_other_binary"
expect "...and one matching only ANOTHER binary's test is dead" 1 "$WORK/bin_wrong" \
    "DEAD FILTER Test: \`a_test_in_the_other_binary\`"

echo
echo "── --skip is deliberately out of scope ──"
#
# A stale `--skip` stops excluding something, so the excluded test starts
# RUNNING: wrong, but loudly, and a leg that then fails says so. Only the silent
# direction is gated. Asserting it keeps the boundary a decision rather than an
# oversight.

# The live filter beside it is load-bearing: an invocation with no filter at all
# never reaches the liveness loop, so a fixture carrying only the `--skip` would
# pass whether or not skips are gated — a case that pins nothing.
make_fixture "$WORK/skip" "cargo test -p demo --lib -- tests:: --skip nothing::matches::this"
expect "a --skip naming nothing is not reported" 0 "$WORK/skip"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "suite-coverage filter self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "suite-coverage filter self-test: $TESTS_RUN/$TESTS_RUN passed"
