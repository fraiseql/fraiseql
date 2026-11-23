#!/usr/bin/env bash
# Unit tests for tools/check-leg-feature-sets.py.
#
# Run directly:  bash tools/tests/leg_feature_sets_test.sh
# Exits non-zero if any assertion fails.
#
# The gate holds each integration leg to one feature list per package (#1886, #1902),
# with a `// feature-off: <flag> — <suite>` annotation for a line that needs features OFF.
# The annotation is checked where it stands, so each shape below is one the gate must
# REJECT on its own: a stale annotation, one above the wrong line, one whose suite is
# gone or not run by its line, one whose line still turns the flag on through another
# feature, or drops more than the flag needs.
# shellcheck disable=SC2016 # Reason: the expected messages quote code in literal backticks.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-leg-feature-sets.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

LEG_SET='arrow,observers,sources'
LEAN_SET='observers'

# fixture <dir> <postgres-body> [<server-leg-features>]
# A tree with the two legs and a fraiseql-server whose `sources` implies
# `functions-runtime`, and a suite with both feature arms.
fixture() {
    local dir="$1" body="$2" leg_features="${3:-$LEG_SET}"
    mkdir -p "$dir/.dagger" "$dir/crates/fraiseql-server/src/server" \
        "$dir/crates/fraiseql-server/tests" "$dir/crates/fraiseql-observers"
    cat >"$dir/crates/fraiseql-server/Cargo.toml" <<'TOML'
[package]
name = "fraiseql-server"

[features]
default = []
arrow = ["dep:fraiseql-arrow"]
observers = ["dep:fraiseql-observers"]
functions-runtime = []
sources = ["functions-runtime", "observers"]
grpc = []
TOML
    printf '[package]\nname = "fraiseql-observers"\n' >"$dir/crates/fraiseql-observers/Cargo.toml"
    cat >"$dir/crates/fraiseql-server/src/server/parity_tests.rs" <<'RS'
#[cfg(not(feature = "arrow"))]
fn a() {}
#[cfg(not(feature = "functions-runtime"))]
fn b() {}
#[cfg(feature = "grpc")]
fn c() {}
RS
    printf '#[test]\nfn plain() {}\n' >"$dir/crates/fraiseql-server/src/server/plain_tests.rs"
    printf '#![cfg(feature = "arrow")]\n' >"$dir/crates/fraiseql-server/tests/flight_pg.rs"
    cat >"$dir/.dagger/main.go" <<GO
package main

const (
	serverTestFeatures = "arrow,observers"
	capabilityFixtureExtraFeatures = "sources"
	serverUnifiedFeatures = "$leg_features"
	serverUnifiedLeanFeatures = "$LEAN_SET"
)

func (m *FraiseqlCi) integrationPostgres(ctx context.Context) (string, error) {
	script := strings.Join([]string{
		"set -e",
$body
	}, "\n")
	return script, nil
}

func (m *FraiseqlCi) integrationObservers(ctx context.Context) (string, error) {
	script := strings.Join([]string{
		"cargo test -p fraiseql-observers --test smtp -- --ignored",
	}, "\n")
	return script, nil
}
GO
}

# assert_gate <name> <expected-exit> <expected-message> <postgres-body> [<server-leg-features>]
# The message pins WHY the gate answered as it did: a rejection for another reason is a
# test of nothing.
assert_gate() {
    local name="$1" want_exit="$2" want_msg="$3" body="$4" leg_features="${5:-$LEG_SET}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    fixture "$dir" "$body" "$leg_features"

    local out rc
    set +e
    out="$(LEG_FEATURE_SETS_ROOT="$dir" python3 "$GATE" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ] || [[ "$out" != *"$want_msg"* ]]; then
        echo "  FAIL: $name — expected exit $want_exit and \"$want_msg\", got $rc"
        printf '        %s\n' "${out//$'\n'/$'\n'        }"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "  ok: $name"
    fi
}

LEG_LINE='		"cargo test -p fraiseql-server --features '"'"'" + serverUnifiedFeatures + "'"'"' --lib -- server::plain_tests:: --test-threads=1",'
LEAN_LINE='		"cargo test -p fraiseql-server --features '"'"'" + serverUnifiedLeanFeatures + "'"'"' --lib -- server::parity_tests:: --test-threads=1",'
OFF_ARROW='		// feature-off: arrow — server::parity_tests'
OFF_FUNCS='		// feature-off: functions-runtime — server::parity_tests'

echo "=== check-leg-feature-sets.py ==="

# ── Accepted ─────────────────────────────────────────────────────────────────────
assert_gate "one-set-and-an-annotated-lean-line-pass" 0 'leg-feature-sets: OK' \
    "$LEG_LINE
$OFF_ARROW
$OFF_FUNCS
$LEAN_LINE"

assert_gate "a-test-target-suite-selected-by---test-passes" 0 'leg-feature-sets: OK' \
    "$LEG_LINE
		// feature-off: arrow — flight_pg
		\"cargo test -p fraiseql-server --features 'observers,sources' --test flight_pg\","

assert_gate "a-filter-naming-a-module-above-the-suite-selects-it" 0 'leg-feature-sets: OK' \
    "$LEG_LINE
$OFF_ARROW
$OFF_FUNCS
		\"cargo test -p fraiseql-server --features '\" + serverUnifiedLeanFeatures + \"' --lib -- server:: --test-threads=1\","

# ── The defect itself ────────────────────────────────────────────────────────────
assert_gate "two-unannotated-lists-for-one-package-are-rejected" 1 '`fraiseql-server` is built with 2 feature lists' \
    "$LEG_LINE
		\"cargo test -p fraiseql-server --features auth --test integration\","

assert_gate "a-canary-line-with-its-own-list-counts-as-a-build" 1 '`fraiseql-server` is built with 2 feature lists' \
    "$LEG_LINE
		\"bash tools/ci-target-canary.sh -- test -p fraiseql-server --features observers --lib\", // #880 canary"

# ── The annotation is checked where it stands ────────────────────────────────────
assert_gate "an-annotation-whose-suite-does-not-exist-is-rejected" 1 'no suite `server::renamed_parity_tests`' \
    "$LEG_LINE
		// feature-off: arrow — server::renamed_parity_tests
		// feature-off: functions-runtime — server::renamed_parity_tests
		\"cargo test -p fraiseql-server --features '\" + serverUnifiedLeanFeatures + \"' --lib -- server::renamed_parity_tests:: --test-threads=1\","

assert_gate "an-annotation-whose-suite-the-line-does-not-run-is-rejected" 1 'the line does not select `server::parity_tests`' \
    "$LEG_LINE
$OFF_ARROW
$OFF_FUNCS
		\"cargo test -p fraiseql-server --features '\" + serverUnifiedLeanFeatures + \"' --lib -- server::plain_tests:: --test-threads=1\","

assert_gate "a-filter-cutting-a-module-name-short-does-not-select-it" 1 'the line does not select `server::parity_tests`' \
    "$LEG_LINE
$OFF_ARROW
$OFF_FUNCS
		\"cargo test -p fraiseql-server --features '\" + serverUnifiedLeanFeatures + \"' --lib -- server::parity --test-threads=1\","

assert_gate "an-annotation-on-a-line-building-the-leg-set-is-rejected" 1 "annotated feature-off, but builds the leg's set" \
    "$LEG_LINE
$OFF_ARROW
		\"cargo test -p fraiseql-server --features '\" + serverUnifiedFeatures + \"' --lib -- server::parity_tests::\","

assert_gate "a-flag-still-on-through-an-implying-feature-is-rejected" 1 'the line still turns `functions-runtime` on' \
    "$LEG_LINE
$OFF_FUNCS
		\"cargo test -p fraiseql-server --features 'arrow,sources' --lib -- server::parity_tests::\","

assert_gate "a-line-dropping-more-than-its-flags-need-is-rejected" 1 "differs by +[] -['observers']" \
    "$LEG_LINE
$OFF_ARROW
		\"cargo test -p fraiseql-server --features sources --lib -- server::parity_tests::\","

assert_gate "a-suite-without-an-arm-on-the-flag-is-rejected" 1 'has no `feature = "arrow"` arm' \
    "$LEG_LINE
		// feature-off: arrow — server::plain_tests
		\"cargo test -p fraiseql-server --features 'observers,sources' --lib -- server::plain_tests::\","

assert_gate "a-flag-the-leg-set-never-turns-on-is-rejected" 1 "is not ON in the leg's set" \
    "$LEG_LINE
$OFF_ARROW
$OFF_FUNCS
		// feature-off: grpc — server::parity_tests
$LEAN_LINE"

assert_gate "a-flag-that-is-no-feature-of-the-package-is-rejected" 1 '`kafka` is not a feature of fraiseql-server' \
    "$LEG_LINE
		// feature-off: kafka — server::parity_tests
$LEAN_LINE"

assert_gate "an-annotation-above-a-non-cargo-line-is-rejected" 1 'stands above no cargo line' \
    "$LEG_LINE
$OFF_ARROW
		\"echo 'between'\",
$LEAN_LINE"

assert_gate "an-annotation-on-the-neighbouring-line-does-not-cover-this-one" 1 'the line does not select `server::parity_tests`' \
    "$OFF_ARROW
$OFF_FUNCS
$LEG_LINE
$LEAN_LINE"

# ── The superset holds to its sources ────────────────────────────────────────────
assert_gate "a-leg-set-missing-a-feature-of-its-sources-is-rejected" 1 "\`serverUnifiedFeatures\` lacks ['sources']" \
    "$LEG_LINE" 'arrow,observers'

# ── A gate that reads nothing passes everything ──────────────────────────────────
assert_gate "an-unresolvable-const-is-fatal" 2 'cannot resolve `noSuchConst`' \
    "		\"cargo test -p fraiseql-server --features '\" + noSuchConst + \"' --lib\","

assert_gate "a-leg-with-no-cargo-line-is-fatal" 2 'no cargo line read' \
    "		\"echo 'nothing here'\","

# ── The real tree ────────────────────────────────────────────────────────────────
TESTS_RUN=$((TESTS_RUN + 1))
if python3 "$GATE" >/dev/null 2>&1; then
    echo "  ok: the-repository-passes"
else
    echo "  FAIL: the-repository-passes"
    python3 "$GATE" 2>&1 | sed 's/^/        /' || true
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo ""
echo "leg_feature_sets_test: $((TESTS_RUN - TESTS_FAILED))/$TESTS_RUN passed"
[ "$TESTS_FAILED" -eq 0 ]
