#!/usr/bin/env bash
# Unit tests for tools/check-suite-coverage.py: a suite gated on a feature its crate does
# not define is run by no leg, `--all-features` included (#2619).
#
# Run directly:  bash tools/tests/suite_coverage_undefined_feature_test.sh
# Exits non-zero if any assertion fails.
#
# `--all-features` turns on every feature the crate DEFINES. A target whose
# `required-features` names anything else is never built, and a `cfg(feature = …)` on
# such a name compiles its code out, under every invocation. The gate answered "covered"
# for any `--all-features` leg without asking whether the feature exists.
#
# The defined set is the crate's `[features]` keys plus the implicit feature of each
# optional dependency that no feature names as `dep:<name>`. U3 and U8 keep the fix from
# flagging a suite that does run.
#
# No Rust toolchain and no cargo: the gate reads manifests and source as text.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-suite-coverage.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <manifest tail> <tests/probe.rs body> [<src/tests.rs body> <mod attrs>]
#
# One crate whose manifest ends with <manifest tail> (its [features], dependencies and
# [[test]] entries), one test binary, and a Dagger `Test` leg running
# `cargo test -p demo --all-features`, reported by a required context. The verdict
# turns on the manifest and the source alone.
make_fixture() {
    local dir="$1" tail="$2" probe="$3" lib_tests="${4:-}" mod_attrs="${5:-#[cfg(test)]}"
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
    printf 'required = ["gates"]\n' >"$dir/tools/required-checks.toml"
    printf '[workspace]\nmembers = ["crates/demo"]\n' >"$dir/Cargo.toml"

    cat >"$dir/crates/demo/Cargo.toml" <<TOML
[package]
name = "demo"
version = "0.0.0"
edition = "2021"

$tail
TOML

    if [ -n "$lib_tests" ]; then
        printf 'pub fn thing() -> u8 { 1 }\n\n%s\nmod tests;\n' "$mod_attrs" \
            >"$dir/crates/demo/src/lib.rs"
        printf '%s\n' "$lib_tests" >"$dir/crates/demo/src/tests.rs"
    else
        printf 'pub fn thing() -> u8 { 1 }\n' >"$dir/crates/demo/src/lib.rs"
    fi
    printf '%s\n' "$probe" >"$dir/crates/demo/tests/probe.rs"

    cat >"$dir/.dagger/main.go" <<'GO'
package main

func (m *FraiseqlCi) Test() string {
	script := []string{
		"cargo test -p demo --all-features",
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

PROBE='#[test]
fn a_thing_holds() {}'

FEATURES='[features]
defined = []'

# U1 (RED before #2619): `required-features` names a feature the crate does not define.
make_fixture "$WORK/u1" "$FEATURES

[[test]]
name = \"probe\"
required-features = [\"undefined\"]" "$PROBE"
expect "U1 a target requiring an undefined feature is run by no leg" 1 "$WORK/u1" \
    "ORPHAN demo::probe"

# U2: the control. A defined feature is on under --all-features.
make_fixture "$WORK/u2" "$FEATURES

[[test]]
name = \"probe\"
required-features = [\"defined\"]" "$PROBE"
expect "U2 a target requiring a defined feature is covered" 0 "$WORK/u2"

# U3: an optional dependency's implicit feature is defined.
make_fixture "$WORK/u3" "$FEATURES

[dependencies]
serde = { version = \"1\", optional = true }

[[test]]
name = \"probe\"
required-features = [\"serde\"]" "$PROBE"
expect "U3 an optional dependency's implicit feature is defined" 0 "$WORK/u3"

# U4: a feature that names the dependency as `dep:serde` removes the implicit feature.
make_fixture "$WORK/u4" "[features]
defined = [\"dep:serde\"]

[dependencies]
serde = { version = \"1\", optional = true }

[[test]]
name = \"probe\"
required-features = [\"serde\"]" "$PROBE"
expect "U4 a dependency named as dep: has no implicit feature" 1 "$WORK/u4" "ORPHAN demo::probe"

# U5: a file-level cfg on an undefined feature, no `required-features`.
make_fixture "$WORK/u5" "$FEATURES" '#![cfg(feature = "undefined")]

#[test]
fn a_thing_holds() {}'
expect "U5 a binary gated by a file-level cfg on an undefined feature is run by no leg" 1 \
    "$WORK/u5" "ORPHAN demo::probe"

# U6: a lib test module gated on an undefined feature.
make_fixture "$WORK/u6" "$FEATURES" "$PROBE" '#[test]
fn inside() {}' '#[cfg(all(test, feature = "undefined"))]'
expect "U6 a lib test module gated on an undefined feature is run by no leg" 1 "$WORK/u6" \
    "ORPHAN demo::lib::tests"

# U7: a lib test module behind `any(...)` over undefined features only. (A file-level
# `any(...)` is refused outright, so the lib module is where the gate models one.)
make_fixture "$WORK/u7" "$FEATURES" "$PROBE" '#[test]
fn inside() {}' '#[cfg(all(test, any(feature = "undefined", feature = "also-undefined")))]'
expect "U7 an any() of undefined features is satisfied by no leg" 1 "$WORK/u7" \
    "ORPHAN demo::lib::tests"

# U8: the same module with one defined member in the group is covered.
make_fixture "$WORK/u8" "$FEATURES" "$PROBE" '#[test]
fn inside() {}' '#[cfg(all(test, any(feature = "undefined", feature = "defined")))]'
expect "U8 an any() with a defined member is covered" 0 "$WORK/u8"

echo
echo "suite-coverage undefined-feature self-test: $((TESTS_RUN - TESTS_FAILED))/$TESTS_RUN passed"
[ "$TESTS_FAILED" -eq 0 ]
