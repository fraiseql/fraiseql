#!/usr/bin/env bash
# Unit tests for the inner-feature-gate side of tools/check-suite-coverage.py (#1179).
#
# Run directly:  bash tools/tests/suite_coverage_inner_gates_test.sh
# Exits non-zero if any assertion fails.
#
# The gate used to read feature gates only off the `mod tests;` DECLARATION chain.
# A test fn behind `#[cfg(feature = "x")]` inside an UNGATED `mod tests` was
# therefore invisible: the module always compiles, so the gate counted it covered
# while every test in it was compiled out. #1179 is that shape — and inverted from
# how it was filed, since the arrow leg does enable `parquet` while nothing
# compiled the `not(feature = "parquet")` refusal arms.
#
# Both directions matter and they are not symmetric:
#
#   * a MISSED gate is a suite that reads green while running nothing;
#   * a FALSE gate costs a real leg line — the first draft of this discovery
#     attributed a `#[cfg(feature = "transforms")] mod render_tests` to its parent
#     `routes::tests`, so a leg already filtering on `routes::tests::render_tests`
#     was reported as not covering it. That fixture is F1 below.
#
# No Rust toolchain and no cargo: the gate reads source as text.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-suite-coverage.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <crate-features-toml> <tests.rs body> <dagger cargo line>
# A minimal repo: one crate with a lib, an UNGATED `mod tests`, and one Dagger
# invocation. Everything the assertion turns on is in the tests.rs body and the
# invocation, so a verdict is attributable to those two alone.
make_fixture() {
    local dir="$1" features="$2" body="$3" invocation="$4"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/crates/demo/src"
    cp "$GATE" "$dir/tools/check-suite-coverage.py"

    cat >"$dir/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/demo"]
TOML

    cat >"$dir/crates/demo/Cargo.toml" <<TOML
[package]
name = "demo"
version = "0.0.0"
edition = "2021"

[features]
$features
TOML

    # `mod tests` is deliberately UNGATED: the whole point is that the module-chain
    # scan sees nothing here and the inner scan must.
    cat >"$dir/crates/demo/src/lib.rs" <<'RS'
pub fn thing() -> u8 { 1 }

#[cfg(test)]
mod tests;
RS

    printf '%s\n' "$body" >"$dir/crates/demo/src/tests.rs"

    cat >"$dir/.dagger/main.go" <<GO
package main

func (m *FraiseqlCi) Test() string {
	script := []string{
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

PLAIN_BODY='#[test]
fn always_runs() {}

#[cfg(feature = "parquet")]
#[test]
fn only_with_parquet() {}'

NEG_BODY='#[test]
fn always_runs() {}

#[cfg(not(feature = "parquet"))]
#[test]
fn refusal_path_names_the_feature() {}'

ANY_BODY='#[test]
fn always_runs() {}

#[cfg(any(feature = "csv", feature = "xlsx"))]
#[test]
fn one_of_the_writers_is_on() {}'

ALL_BODY='#[test]
fn always_runs() {}

#[cfg(all(feature = "csv", feature = "xlsx"))]
#[test]
fn both_writers_are_on() {}'

SUBMOD_BODY='#[test]
fn always_runs() {}

#[cfg(feature = "transforms")]
mod render_tests {
    #[test]
    fn renders() {}
}'

FEATS='parquet = []
csv = []
xlsx = []
transforms = []'

echo "suite-coverage inner feature gates"
echo

echo "── a positive inner gate is found, and satisfied by a leg that enables it ──"
make_fixture "$WORK/p_missing" "$FEATS" "$PLAIN_BODY" "cargo test -p demo --lib"
expect "an inner #[cfg(feature)] with no leg enabling it is an ORPHAN" 1 "$WORK/p_missing" "demo::lib::tests[parquet]"

make_fixture "$WORK/p_ok" "$FEATS" "$PLAIN_BODY" "cargo test -p demo --lib --features parquet"
expect "...and is covered once a leg enables it" 0 "$WORK/p_ok"

make_fixture "$WORK/p_all" "$FEATS" "$PLAIN_BODY" "cargo test -p demo --lib --all-features"
expect "...--all-features covers it too" 0 "$WORK/p_all"

echo
echo "── a not(feature) gate needs a leg with the feature OFF (the #1179 defect) ──"
# This is the one that had never been modelled for inner gates: every lib
# invocation turns features ON, and `--all-features` compiles a not() arm to
# nothing, so the refusal-path assertions ran nowhere.
make_fixture "$WORK/n_all" "$FEATS" "$NEG_BODY" "cargo test -p demo --lib --all-features"
expect "--all-features does NOT cover a not(feature) arm" 1 "$WORK/n_all" "requires OFF=['parquet']"

make_fixture "$WORK/n_on" "$FEATS" "$NEG_BODY" "cargo test -p demo --lib --features parquet"
expect "...nor does a leg that enables the feature" 1 "$WORK/n_on" "requires OFF=['parquet']"

make_fixture "$WORK/n_off" "$FEATS" "$NEG_BODY" "cargo test -p demo --lib"
expect "...a default-features run covers it" 0 "$WORK/n_off"

echo
echo "── any(...) is a disjunction, modelled rather than dropped ──"
# Dropping a predicate the gate cannot read is how it would report coverage it
# never checked, so `any(a, b)` is resolved as "at least one".
make_fixture "$WORK/a_none" "$FEATS" "$ANY_BODY" "cargo test -p demo --lib"
expect "neither member enabled ⇒ ORPHAN" 1 "$WORK/a_none" "demo::lib::tests"

make_fixture "$WORK/a_one" "$FEATS" "$ANY_BODY" "cargo test -p demo --lib --features csv"
expect "one member is enough" 0 "$WORK/a_one"

echo
echo "── all(...) needs every member ──"
make_fixture "$WORK/all_one" "$FEATS" "$ALL_BODY" "cargo test -p demo --lib --features csv"
expect "one of two ⇒ ORPHAN" 1 "$WORK/all_one" "demo::lib::tests"

make_fixture "$WORK/all_both" "$FEATS" "$ALL_BODY" "cargo test -p demo --lib --features csv,xlsx"
expect "both ⇒ covered" 0 "$WORK/all_both"

echo
echo '── a gated inner mod is attributed to ITS path, not its parent'"'"'s ──'
# F1: the false-positive direction. A leg filtering on the inner module's own path
# genuinely covers it; attributing the tests to the parent `tests` module instead
# reported an orphan that was covered, which costs a real leg line.
make_fixture "$WORK/f1" "$FEATS" "$SUBMOD_BODY" "cargo test -p demo --lib --features transforms -- tests::render_tests"
expect "a filter naming the gated submodule covers it" 0 "$WORK/f1"

make_fixture "$WORK/f2" "$FEATS" "$SUBMOD_BODY" "cargo test -p demo --lib --features transforms -- tests::other_tests"
expect "...and a filter naming a DIFFERENT module does not" 1 "$WORK/f2" "demo::lib::tests::render_tests[transforms]"

echo
echo "── an ungated module with no inner gates creates no target at all ──"
make_fixture "$WORK/none" "$FEATS" '#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "no feature cfgs ⇒ nothing to track" 0 "$WORK/none"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "suite-coverage inner-gate self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "suite-coverage inner-gate self-test: $TESTS_RUN/$TESTS_RUN passed"
