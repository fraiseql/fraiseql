#!/usr/bin/env bash
# Red-capability pin for tools/check-ignored-lines.py.
#
# Run directly:  bash tools/tests/ignored_lines_test.sh
# Exits non-zero if any assertion fails.
#
# The check exists because `cargo test … -- --ignored` on a target with no `#[ignore]`
# test runs 0 tests and exits 0 (#2103): five Makefile lines did. So the cases below are
# the ways the check could pass such a line: an `#[ignore]` that only appears in a comment
# or a string, a glob or a `--lib` scope read wrong, a Makefile line hidden behind an
# environment prefix, a `main.go` line, and a Makefile the parser reads nothing from.
#
# Each case runs the real check over a fixture repo: the two tools, one crate `fx`, a
# minimal .dagger/main.go and a Makefile.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <makefile recipe line> [main.go command]
make_fixture() {
    local dir="$1" recipe="$2" dagger="${3:-cargo test -p fx --test has_ignored -- --ignored --test-threads=1}"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/.github/workflows" "$dir/crates/fx/src" "$dir/crates/fx/tests"
    cp "$REPO_ROOT/tools/check-suite-coverage.py" "$REPO_ROOT/tools/check-ignored-lines.py" "$dir/tools/"
    printf '[package]\nname = "fx"\nversion = "0.1.0"\nedition = "2021"\n' >"$dir/crates/fx/Cargo.toml"
    printf '#[cfg(test)]\nmod tests {\n    #[test]\n    #[ignore = "needs a service"]\n    fn lib_ignored() {}\n}\n' \
        >"$dir/crates/fx/src/lib.rs"
    printf '#[test]\n#[ignore]\nfn a() {}\n' >"$dir/crates/fx/tests/has_ignored.rs"
    # An `#[ignore]` written in a comment and in a string marks no test.
    printf '#[test]\nfn b() {}\n// #[ignore] in a comment\nconst S: &str = "#[ignore]";\n' \
        >"$dir/crates/fx/tests/no_ignored.rs"
    cat >"$dir/.dagger/main.go" <<GO
package main

func (m *FraiseqlCi) leg(ctx context.Context) (string, error) {
	script := strings.Join([]string{
		"$dagger",
	}, "\n")
	return script, nil
}
GO
    printf 'demo:\n%b\n' "$recipe" >"$dir/Makefile"
}

# run_case <id> <makefile recipe line> [main.go command]
run_case() {
    local id="$1"; shift
    local dir="$WORK/$id" rc
    make_fixture "$dir" "$@"
    set +e
    ( cd "$dir" && python3 tools/check-ignored-lines.py >"$dir/.out" 2>&1 )
    rc=$?
    set -e
    echo "$rc" >"$dir/.rc"
}

# check <id> <description> <shell condition, run in the case's dir>
check() {
    local id="$1" desc="$2" cond="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    if ( cd "$WORK/$id" && eval "$cond" ); then
        printf '  ✅ %-10s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-10s %s\n' "$id" "$desc"
        sed 's/^/        /' "$WORK/$id/.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "check-ignored-lines.py"

run_case honest '\tDATABASE_URL="x" \\\n\t\tcargo test -p fx --test has_ignored -- --ignored'
check honest "lines that each select an #[ignore] test: exit 0" '[ "$(cat .rc)" = 0 ]'
check honest "and it counts the main.go line and the Makefile line" 'grep -q "all 2 " .out'

run_case maketest '\tDATABASE_URL="x" \\\n\t\tcargo test -p fx --test no_ignored -- --ignored'
check maketest "a Makefile line on a target with no #[ignore] test: exit 1" '[ "$(cat .rc)" = 1 ]'
check maketest "naming its Makefile line" 'grep -q "Makefile:3" .out'
check maketest "an #[ignore] in a comment or a string does not count" 'grep -q "no_ignored" .out'

run_case envline '\tREDIS_URL=redis://x cargo test -p fx --test no_ignored -- --ignored'
check envline "a VAR=value prefix on the same line is still read: exit 1" '[ "$(cat .rc)" = 1 ]'

run_case dagger '\tcargo test -p fx --test has_ignored -- --ignored' \
    'cargo test -p fx --test no_ignored -- --ignored --test-threads=1'
check dagger "a main.go line on a target with no #[ignore] test: exit 1" \
    '[ "$(cat .rc)" = 1 ] && grep -q "main.go leg" .out'

run_case globred "\tcargo test -p fx --test 'no_*' -- --ignored"
check globred "a --test glob matching only targets without one: exit 1" '[ "$(cat .rc)" = 1 ]'

run_case globok "\tcargo test -p fx --test 'has_*' -- --ignored"
check globok "a --test glob matching a target with one: exit 0" '[ "$(cat .rc)" = 0 ]'

run_case libok '\tcargo test -p fx --lib -- --ignored'
check libok "--lib, with an #[ignore] test in src/: exit 0" '[ "$(cat .rc)" = 0 ]'

run_case libred '\tcargo test -p fx --lib -- --ignored'
# The attribute moved into a comment: what is left marks no test.
sed -i 's|#\[ignore = "needs a service"\]|// #[ignore = "needs a service"]|' "$WORK/libred/crates/fx/src/lib.rs"
( cd "$WORK/libred" && python3 tools/check-ignored-lines.py >.out 2>&1; echo $? >.rc ) || true
check libred "--lib, with an #[ignore] only in a comment in src/: exit 1" '[ "$(cat .rc)" = 1 ]'

run_case include '\tcargo test -p fx --test has_ignored -- --ignored' \
    'cargo test -p fx --test no_ignored -- --include-ignored --test-threads=1'
check include "--include-ignored runs the plain tests too, so it is not checked" '[ "$(cat .rc)" = 0 ]'

run_case nolines '\tcargo test -p fx --test has_ignored'
check nolines "a Makefile with no --ignored line is an error: exit 2" '[ "$(cat .rc)" = 2 ]'

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ $TESTS_RUN assertions passed"
