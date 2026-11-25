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
#              [<env binding>] [<attributes on the `mod tests;` declaration>]
# A minimal repo: one crate with a lib, an UNGATED `mod tests`, and one Dagger
# invocation. Everything the assertion turns on is in the tests.rs body and the
# invocation, so a verdict is attributable to those two alone. The sixth argument
# replaces the declaration's `#[cfg(test)]`, for the cases that gate the module itself.
make_fixture() {
    local dir="$1" features="$2" body="$3" invocation="$4" env_binding="${5:-}"
    local mod_attrs="${6:-#[cfg(test)]}"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/crates/demo/src" "$dir/.github/workflows"
    cp "$GATE" "$dir/tools/check-suite-coverage.py"

    # #1289: the gate now also asks whether a covering leg can fail a merge, so a
    # fixture has to say how its Dagger leg reaches CI. One workflow calling
    # `dagger call test`, and a mirror declaring that job's context required —
    # which keeps every verdict below attributable to the feature gates under test
    # rather than to a missing merge gate.
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

    cat >"$dir/crates/demo/Cargo.toml" <<TOML
[package]
name = "demo"
version = "0.0.0"
edition = "2021"

[features]
$features
TOML

    # `mod tests` is UNGATED unless a case passes its own attributes: the point of
    # most cases is that the module-chain scan sees nothing here and the inner scan must.
    printf 'pub fn thing() -> u8 { 1 }\n\n%s\nmod tests;\n' "$mod_attrs" \
        >"$dir/crates/demo/src/lib.rs"

    printf '%s\n' "$body" >"$dir/crates/demo/src/tests.rs"

    # The optional fifth argument binds a service to the leg. `inv.env` is read
    # out of the `WithEnvVariable(...)` calls inside the Go function, exactly as
    # `.dagger/main.go` writes them, so a fixture leg can be made to bind a
    # service or deliberately not to (#1297).
    local binding=""
    if [ -n "$env_binding" ]; then
        binding="	_ = ctr.WithEnvVariable(\"$env_binding\", \"stub\")"
    fi
    cat >"$dir/.dagger/main.go" <<GO
package main

func (m *FraiseqlCi) Test() string {
$binding
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
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -cF -- "$needle" >/dev/null; then
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
echo "── the service axis: a leg binding nothing covers nothing ──"

# ── S. The service axis: a leg that binds nothing covers nothing ───────────
#
# `covers_binary` has asked "does this leg bind the services the suite needs?"
# since #960. `covers_module` did not, so ANY leg compiling a module with the
# right features was credited — including one with no service at all, where the
# tests self-skip and read exactly like passes. That is #960 one level down:
# there a suite ran in a leg that could not fail a merge, here a suite is
# credited to a leg that cannot execute it (#1297).
SERVICE_BODY='#[test]
fn always_runs() {}

#[cfg(feature = "parquet")]
#[test]
fn talks_to_redis() {
    let _ = std::env::var("REDIS_URL");
}'

make_fixture "$WORK/svc_none" "$FEATS" "$SERVICE_BODY" "cargo test -p demo --lib --features parquet"
expect "a module needing a service is not covered by a leg binding none" 1 \
    "$WORK/svc_none" "demo::lib::tests[parquet]"

make_fixture "$WORK/svc_bound" "$FEATS" "$SERVICE_BODY" \
    "cargo test -p demo --lib --features parquet" "REDIS_URL"
expect "...and is covered once the leg binds it" 0 "$WORK/svc_bound"

make_fixture "$WORK/svc_wrong" "$FEATS" "$SERVICE_BODY" \
    "cargo test -p demo --lib --features parquet" "DATABASE_URL"
expect "...a DIFFERENT service does not satisfy it" 1 "$WORK/svc_wrong" \
    "demo::lib::tests[parquet]"

# A leg binds what a method it calls binds, and only a method it calls.
make_fixture "$WORK/svc_helper" "$FEATS" "$SERVICE_BODY" \
    "cargo test -p demo --lib --features parquet"
cat >>"$WORK/svc_helper/.dagger/main.go" <<'GO'

func (m *FraiseqlCi) withRedis(ctr *Container) *Container {
	return ctr.WithEnvVariable("REDIS_URL", "stub")
}
GO
cp -r "$WORK/svc_helper" "$WORK/svc_helper_uncalled"
sed -i 's/^\tscript := \[\]string{$/\t_ = m.withRedis(nil)\n&/' "$WORK/svc_helper/.dagger/main.go"
expect "...bound by a method the leg calls, it is covered" 0 "$WORK/svc_helper"
expect "...bound by a method the leg never calls, it is not" 1 "$WORK/svc_helper_uncalled" \
    "demo::lib::tests[parquet]"

# The service is read from the gated span, not from the file. Whole-file
# detection would call every target in a tests.rs service-bound because one test
# somewhere in it dials Redis — it read `commands::tests::run_tests[run-server]`
# as needing Postgres and Redis when its own 23 tests touch neither, and an
# over-demanding gate gets an exemption written for it.
SPLIT_BODY='#[test]
fn always_runs() {
    let _ = std::env::var("REDIS_URL");
}

#[cfg(feature = "parquet")]
#[test]
fn touches_nothing() {}'

make_fixture "$WORK/svc_split" "$FEATS" "$SPLIT_BODY" "cargo test -p demo --lib --features parquet"
expect "...and a service used OUTSIDE the gate is not the gate's need" 0 "$WORK/svc_split"

echo
echo "── the #[ignore] axis, at the granularity of the gate ──"

# ── I. The #[ignore] axis, at the granularity of the gate ──────────────────
#
# `-- --ignored` runs ONLY #[ignore]d tests and a plain run skips them, so each
# is a leg that executes none of the other's tests. Both directions have a live
# instance: `integration (redis)` runs `-p fraiseql-observers --lib -- --ignored`
# over modules with no ignored test at all, and the four `redis-pkce` tests are
# all #[ignore]d while the only leg compiling them runs plain.
IGNORED_BODY='#[test]
fn always_runs() {}

#[cfg(feature = "parquet")]
#[test]
#[ignore = "needs the service"]
fn only_when_asked() {}'

make_fixture "$WORK/ign_plain" "$FEATS" "$IGNORED_BODY" "cargo test -p demo --lib --features parquet"
expect "an all-#[ignore]d module is not covered by a plain run" 1 "$WORK/ign_plain" \
    "demo::lib::tests[parquet]"

make_fixture "$WORK/ign_ignored" "$FEATS" "$IGNORED_BODY" \
    "cargo test -p demo --lib --features parquet -- --ignored"
expect "...and is covered by an --ignored run" 0 "$WORK/ign_ignored"

make_fixture "$WORK/ign_include" "$FEATS" "$IGNORED_BODY" \
    "cargo test -p demo --lib --features parquet -- --include-ignored"
expect "...and by --include-ignored" 0 "$WORK/ign_include"

make_fixture "$WORK/ign_none" "$FEATS" "$PLAIN_BODY" \
    "cargo test -p demo --lib --features parquet -- --ignored"
expect "an --ignored run covers nothing in a module with no #[ignore]" 1 \
    "$WORK/ign_none" "demo::lib::tests[parquet]"

# The counts come from the gated span too, and from source with string literals
# blanked. `fraiseql-auth/src/tests.rs` holds 345 tests of which 9 look ignored —
# one of the nine is the text `#[ignore]d` inside a message string — so whole-file
# counting says "not ignore-only" over a gate whose every test is ignored.
MIXED_BODY='#[test]
fn always_runs() {}

#[test]
#[ignore = "unrelated"]
fn ungated_and_ignored() {}

#[cfg(feature = "parquet")]
#[test]
fn gated_and_not_ignored() {
    assert_eq!("#[ignore]", "#[ignore]");
}'

make_fixture "$WORK/ign_span" "$FEATS" "$MIXED_BODY" "cargo test -p demo --lib --features parquet"
expect "a plain run covers a gate whose own tests are not ignored" 0 "$WORK/ign_span"

echo
echo "── an ungated module with no inner gates creates no target at all ──"

make_fixture "$WORK/none" "$FEATS" '#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "no feature cfgs ⇒ nothing to track" 0 "$WORK/none"

echo
echo "── the predicate on a \`mod\` DECLARATION is read as a predicate ──"
#
# The module-chain scan collected every \`feature = "…"\` it saw above a declaration
# and required them all. That over-demands \`any(…)\` (a leg enabling one member was
# told it covered nothing) and reads \`cfg_attr\` as a gate, but it also HIDES suites:
# \`not(feature = "x")\` came out as "requires x", so the one leg that compiles the
# module out was credited with running it, and an attribute split over several lines
# was cut at its first line, which left the module reading as ungated.

DECL_BODY='#[test]
fn always_runs() {}'

make_fixture "$WORK/d_not_on" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib --features parquet" "" \
    '#[cfg(not(feature = "parquet"))]'
expect "a not(feature) module is not covered by a leg enabling the feature" 1 "$WORK/d_not_on" \
    "requires OFF=['parquet']"

make_fixture "$WORK/d_not_off" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg(not(feature = "parquet"))]'
expect "...and is covered by a leg leaving it off" 0 "$WORK/d_not_off"

make_fixture "$WORK/d_split" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib --features csv" "" \
    '#[cfg(all(
    feature = "csv",
    feature = "xlsx",
))]'
expect "a predicate over several lines is read whole" 1 "$WORK/d_split" \
    "features=['csv', 'xlsx']"

make_fixture "$WORK/d_stacked" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg(feature = "csv")]
// A comment between attributes is not the end of the block.
#[allow(dead_code)]'
expect "every attribute stacked above the declaration is read" 1 "$WORK/d_stacked" \
    "features=['csv']"

make_fixture "$WORK/d_any_one" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib --features csv" "" \
    '#[cfg(any(feature = "csv", feature = "xlsx"))]'
expect "an any() module is covered by a leg enabling one member" 0 "$WORK/d_any_one"

make_fixture "$WORK/d_any_none" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg(any(feature = "csv", feature = "xlsx"))]'
expect "...and not by a leg enabling none" 1 "$WORK/d_any_none" "any=[['csv', 'xlsx']]"

make_fixture "$WORK/d_inner" "$FEATS" '#[cfg(feature = "parquet")]
#[test]
fn only_with_parquet() {}' "cargo test -p demo --lib --features parquet" "" \
    '#[cfg(any(feature = "csv", feature = "xlsx"))]'
expect "a gated test inside an any() module needs the module's gate too" 1 "$WORK/d_inner" \
    "demo::lib::tests[parquet]"

make_fixture "$WORK/d_inner_not" "$FEATS" '#[cfg(feature = "csv")]
#[test]
fn only_with_csv() {}' "cargo test -p demo --lib --features csv,parquet" "" \
    '#[cfg(not(feature = "parquet"))]'
expect "...and so does one inside a not(feature) module" 1 "$WORK/d_inner_not" \
    "demo::lib::tests[csv]"

make_fixture "$WORK/d_any_test" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg(any(test, feature = "csv"))]'
expect "any(test, …) holds in every test build" 0 "$WORK/d_any_test"

make_fixture "$WORK/d_cfg_attr" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg_attr(not(feature = "parquet"), allow(dead_code))]'
expect "a cfg_attr gates nothing" 0 "$WORK/d_cfg_attr"

make_fixture "$WORK/d_nested" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib --all-features" "" \
    '#[cfg(any(feature = "csv", all(feature = "xlsx", feature = "parquet")))]'
expect "a predicate the gate cannot model is refused, not guessed" 2 "$WORK/d_nested" \
    "not modelled"

echo
echo "── a module file's own #![cfg(…)] gates it as its declaration would ──"
#
# A \`tests.rs\` that begins \`#![cfg(all(test, feature = "transforms"))]\` compiles to nothing
# without the feature, whatever its declaration says. The gate read only the declaration,
# so the module looked ungated and any lib run was credited with it.

make_fixture "$WORK/i_file_off" "$FEATS" '#![cfg(all(test, feature = "parquet"))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "a file-level cfg is a gate: a leg without the feature runs nothing" 1 "$WORK/i_file_off" \
    "features=['parquet']"

make_fixture "$WORK/i_file_on" "$FEATS" '//! A doc comment above the attribute is not the end of the header.
#![cfg(all(test, feature = "parquet"))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib --features parquet"
expect "...and a leg enabling it covers the module" 0 "$WORK/i_file_on"

make_fixture "$WORK/i_file_inner" "$FEATS" '#![cfg(not(feature = "parquet"))]

#[cfg(feature = "csv")]
#[test]
fn only_with_csv() {}' "cargo test -p demo --lib --features csv,parquet"
expect "...and its gate reaches the gated tests inside" 1 "$WORK/i_file_inner" \
    "demo::lib::tests[csv]"

# One level up: a parent module file's header gates every module below it.
make_fixture "$WORK/i_parent" "$FEATS" '#[test]
fn always_runs() {}' "cargo test -p demo --lib"
mkdir -p "$WORK/i_parent/crates/demo/src/inner"
mv "$WORK/i_parent/crates/demo/src/tests.rs" "$WORK/i_parent/crates/demo/src/inner/tests.rs"
printf 'pub fn thing() -> u8 { 1 }\n\npub mod inner;\n' >"$WORK/i_parent/crates/demo/src/lib.rs"
printf '#![cfg(feature = "parquet")]\n\n#[cfg(test)]\nmod tests;\n' \
    >"$WORK/i_parent/crates/demo/src/inner/mod.rs"
expect "a parent module file's own cfg gates the tests below it" 1 "$WORK/i_parent" \
    "demo::lib::inner::tests"

make_fixture "$WORK/i_file_any" "$FEATS" '#![cfg(any(feature = "csv", feature = "xlsx"))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib --features xlsx"
expect "...read as a predicate, as a declaration is" 0 "$WORK/i_file_any"

make_fixture "$WORK/i_file_any_none" "$FEATS" '#![cfg(any(feature = "csv", feature = "xlsx"))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "...so a leg enabling no member covers nothing" 1 "$WORK/i_file_any_none" \
    "any=[['csv', 'xlsx']]"

make_fixture "$WORK/i_file_stacked" "$FEATS" '#![allow(clippy::unwrap_used)] // Reason: test code
#![cfg(feature = "parquet")]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "every leading inner attribute is read, not only the first" 1 "$WORK/i_file_stacked" \
    "features=['parquet']"

make_fixture "$WORK/i_file_cfg_attr" "$FEATS" '#![cfg_attr(not(feature = "parquet"), allow(dead_code))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "a file-level cfg_attr gates nothing" 0 "$WORK/i_file_cfg_attr"

make_fixture "$WORK/i_file_nested" "$FEATS" '#![cfg(not(any(feature = "csv", feature = "xlsx")))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
expect "a file-level shape the gate cannot model is refused" 2 "$WORK/i_file_nested" \
    "not modelled"

echo
echo "── a test module is found by its declaration, not by its file name ──"
#
# Discovery searched for files named \`tests.rs\`. A \`#[path = "thing_tests.rs"] mod tests;\`,
# the layout \`lint-tests-layout\` asks for, or a \`mod parity_tests;\`, was never tracked, so
# no leg had to run it whatever gated it.

# rename_tests <dir> <new file name>: the module's file, under another name.
rename_tests() {
    mv "$1/crates/demo/src/tests.rs" "$1/crates/demo/src/$2"
}

make_fixture "$WORK/p_path_off" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg(test)]
#[cfg(feature = "parquet")]
#[path = "thing_tests.rs"]'
rename_tests "$WORK/p_path_off" thing_tests.rs
expect "a #[path] test module is tracked: a leg without its feature runs nothing" 1 \
    "$WORK/p_path_off" "demo::lib::tests"

make_fixture "$WORK/p_path_on" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib --features parquet" "" \
    '#[cfg(test)]
#[cfg(feature = "parquet")]
#[path = "thing_tests.rs"]'
rename_tests "$WORK/p_path_on" thing_tests.rs
expect "...and a leg enabling it covers it" 0 "$WORK/p_path_on"

make_fixture "$WORK/p_named" "$FEATS" '#![cfg(feature = "parquet")]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
rename_tests "$WORK/p_named" parity_tests.rs
printf 'pub fn thing() -> u8 { 1 }\n\n#[cfg(test)]\nmod parity_tests;\n' >"$WORK/p_named/crates/demo/src/lib.rs"
expect "a test module under another name is tracked" 1 "$WORK/p_named" "demo::lib::parity_tests"

make_fixture "$WORK/p_header" "$FEATS" '#![cfg(all(test, feature = "parquet"))]

#[test]
fn always_runs() {}' "cargo test -p demo --lib"
rename_tests "$WORK/p_header" x_tests.rs
printf 'pub fn thing() -> u8 { 1 }\n\nmod x_tests;\n' >"$WORK/p_header/crates/demo/src/lib.rs"
expect "...and so is one that only its own header makes test-only" 1 "$WORK/p_header" \
    "demo::lib::x_tests"

make_fixture "$WORK/p_child" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib"
printf '#[test]\nfn always_runs() {}\n\n#[cfg(feature = "parquet")]\nmod render_tests;\n' \
    >"$WORK/p_child/crates/demo/src/tests.rs"
mkdir -p "$WORK/p_child/crates/demo/src/tests"
printf '#[test]\nfn renders() {}\n' >"$WORK/p_child/crates/demo/src/tests/render_tests.rs"
expect "a test module declared inside one is tracked with its own gate" 1 "$WORK/p_child" \
    "demo::lib::tests::render_tests"

make_fixture "$WORK/p_helper" "$FEATS" '#[cfg(test)]
pub fn a_fixture() -> u8 { 1 }' "cargo test -p demo --lib" "" '#[cfg(feature = "parquet")]'
rename_tests "$WORK/p_helper" support.rs
printf 'pub fn thing() -> u8 { 1 }\n\n#[cfg(feature = "parquet")]\nmod support;\n' \
    >"$WORK/p_helper/crates/demo/src/lib.rs"
expect "a gated module that is not test-only and holds no test is not a test module" 0 \
    "$WORK/p_helper"

make_fixture "$WORK/p_fixtures" "$FEATS" 'pub fn a_fixture() -> u8 { 1 }' "cargo test -p demo --lib"
rename_tests "$WORK/p_fixtures" fixtures.rs
printf 'pub fn thing() -> u8 { 1 }\n\n#[cfg(test)]\n#[cfg(feature = "parquet")]\nmod fixtures;\n' \
    >"$WORK/p_fixtures/crates/demo/src/lib.rs"
expect "a test-only module that holds no test is not a test module" 0 "$WORK/p_fixtures"

make_fixture "$WORK/p_unreached" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib"
rename_tests "$WORK/p_unreached" orphan_tests.rs
printf '#![cfg(feature = "parquet")]\n#[test]\nfn never_compiled() {}\n' >"$WORK/p_unreached/crates/demo/src/orphan_tests.rs"
printf 'pub fn thing() -> u8 { 1 }\n' >"$WORK/p_unreached/crates/demo/src/lib.rs"
expect "a file no declaration reaches is not a module" 0 "$WORK/p_unreached"

echo
echo "── a filter naming a test fn runs the gated code inside it, and only that ──"
#
# \`covers_module\` matched a filter only as a prefix of the target's module path, so a leg
# naming \`tests::plans\` was never credited with a \`#[cfg(feature = "parquet")] { … }\`
# block inside \`plans\`, which it does run.

FN_BODY='#[test]
fn plans() {
    #[cfg(feature = "parquet")]
    {
        let _ = 1;
    }
}

#[test]
fn sibling() {}'

make_fixture "$WORK/fn_named" "$FEATS" "$FN_BODY" "cargo test -p demo --lib --features parquet -- tests::plans"
expect "a leg naming the test fn covers the gated block inside it" 0 "$WORK/fn_named"

make_fixture "$WORK/fn_sibling" "$FEATS" "$FN_BODY" "cargo test -p demo --lib --features parquet -- tests::sibling"
expect "...and a leg naming a sibling fn does not" 1 "$WORK/fn_sibling" "demo::lib::tests[parquet]"

make_fixture "$WORK/fn_gated" "$FEATS" '#[cfg(feature = "parquet")]
#[test]
fn only_with_parquet() {}

#[test]
fn sibling() {}' "cargo test -p demo --lib --features parquet -- tests::only_with_parquet"
expect "a leg naming a gated test fn covers it" 0 "$WORK/fn_gated"

make_fixture "$WORK/fn_two_sites" "$FEATS" '#[test]
fn plans() {
    #[cfg(feature = "parquet")]
    let _ = 1;
}

#[test]
fn renders() {
    #[cfg(feature = "parquet")]
    let _ = 2;
}' "cargo test -p demo --lib --features parquet -- tests::plans"
expect "a gate with sites in two fns needs both named" 1 "$WORK/fn_two_sites" "demo::lib::tests[parquet]"

make_fixture "$WORK/fn_both_sites" "$FEATS" '#[test]
fn plans() {
    #[cfg(feature = "parquet")]
    let _ = 1;
}

#[test]
fn renders() {
    #[cfg(feature = "parquet")]
    let _ = 2;
}' "cargo test -p demo --lib --features parquet -- tests::plans tests::renders"
expect "...and is covered when both are" 0 "$WORK/fn_both_sites"

make_fixture "$WORK/fn_held" "$FEATS" '#[cfg(feature = "parquet")]
mod render {
    #[test]
    fn plans() {}

    #[test]
    fn crops() {}
}' "cargo test -p demo --lib --features parquet -- tests::render::plans"
expect "a gated item holding two tests is not covered by naming one" 1 "$WORK/fn_held" \
    "demo::lib::tests::render[parquet]"

make_fixture "$WORK/fn_helper" "$FEATS" '#[cfg(feature = "parquet")]
fn helper() -> u8 { 1 }

#[test]
fn plans() {}' "cargo test -p demo --lib --features parquet -- tests::plans"
expect "a gated helper outside any test is not covered by naming a test" 1 "$WORK/fn_helper" \
    "demo::lib::tests[parquet]"

make_fixture "$WORK/fn_inline" "$FEATS" 'mod inner {
    #[test]
    fn plans() {
        #[cfg(feature = "parquet")]
        let _ = 1;
    }
}' "cargo test -p demo --lib --features parquet -- tests::inner::plans"
expect "a test fn inside an inline mod is named with its path" 0 "$WORK/fn_inline"

echo
echo "── a gated mod inside an inline mod is named by its whole path ──"

NESTED_BODY='mod outer {
    #[cfg(feature = "parquet")]
    mod gated {
        #[test]
        fn probe() {}
    }

    #[test]
    fn other() {}
}'

make_fixture "$WORK/n_real" "$FEATS" "$NESTED_BODY" \
    "cargo test -p demo --lib --features parquet -- tests::outer::gated::"
expect "a filter on the gated mod's real path covers it" 0 "$WORK/n_real"

make_fixture "$WORK/n_short" "$FEATS" "$NESTED_BODY" \
    "cargo test -p demo --lib --features parquet -- tests::gated::"
expect "...and one on its own name alone does not, and names the real path" 1 "$WORK/n_short" \
    "demo::lib::tests::outer::gated[parquet]"

make_fixture "$WORK/n_exempt" "$FEATS" "$NESTED_BODY" "cargo test -p demo --lib"
printf '[[exempt]]\ntarget = "demo::lib::tests::outer::gated[parquet]"\nreason = "never run by any leg (#0000)"\n' \
    >"$WORK/n_exempt/tools/suite-coverage-exemptions.toml"
expect "an exemption names a nested gated mod by its real path" 0 "$WORK/n_exempt" "1 exemptions in use"

echo
echo "── an exemption for a module a leg runs is refused ──"

make_fixture "$WORK/x_covered" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib --features parquet" "" \
    '#[cfg(feature = "parquet")]'
printf '[[exempt]]\ntarget = "demo::lib::tests"\nreason = "never run by any leg (#0000)"\n' \
    >"$WORK/x_covered/tools/suite-coverage-exemptions.toml"
expect "an [[exempt]] row for a module a leg runs is refused" 1 "$WORK/x_covered" \
    "UNNEEDED EXEMPTION demo::lib::tests"

make_fixture "$WORK/x_needed" "$FEATS" "$DECL_BODY" "cargo test -p demo --lib" "" \
    '#[cfg(feature = "parquet")]'
printf '[[exempt]]\ntarget = "demo::lib::tests"\nreason = "never run by any leg (#0000)"\n' \
    >"$WORK/x_needed/tools/suite-coverage-exemptions.toml"
expect "...and one for a module no leg runs is kept" 0 "$WORK/x_needed" "1 exemptions in use"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "suite-coverage inner-gate self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "suite-coverage inner-gate self-test: $TESTS_RUN/$TESTS_RUN passed"
