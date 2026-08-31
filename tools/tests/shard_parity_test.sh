#!/usr/bin/env bash
# Unit tests for tools/check-shard-parity.py.
#
# Run directly:  bash tools/tests/shard_parity_test.sh
# Exits non-zero if any assertion fails.
#
# The gate exists because `make test-integration-postgres` (#1169) and
# `make test-leg` (#1257) are hand-maintained copies of line lists in
# `.dagger/main.go`, and the whole point of both targets is to be trusted as
# "what CI runs". A parity gate that cannot itself go red would just be a third
# place the same false assurance lives, so every way the two lists can diverge
# has a fixture below and every one must be reported — including the two that
# make a gate worth *less* than no gate: a shard line the parser cannot classify,
# and a shard literal it reads as empty.
#
# Every fixture carries BOTH shard/target pairs, because the gate checks both and
# a fixture that satisfied only one would report the other as missing on every
# assertion — a red that says nothing about the thing under test.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-shard-parity.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <pg-recipe> <pg-entries> [test-recipe] [test-entries]
# Builds a minimal tree with the same shapes the real files use: target-specific
# variable lines before the rule, `@`-prefixed recipe lines, and Go script
# literals whose feature lists are assembled from consts. The `Test` pair
# defaults to one that agrees, so a fixture can vary one shard at a time.
make_fixture() {
    local dir="$1" pg_recipe="$2" pg_entries="$3"
    local test_recipe="${4:-$TEST_RECIPE_OK}" test_entries="${5:-$TEST_ENTRIES_OK}"
    mkdir -p "$dir/.dagger"

    {
        printf 'test-integration-postgres: export DATABASE_URL := postgresql://u:p@localhost:5433/d\n'
        printf 'test-integration-postgres: export RUST_LOG := debug\n'
        printf '.PHONY: test-integration-postgres\n'
        printf 'test-integration-postgres: db-up db-failover-reset\n'
        printf '\t@echo "### core sweep"\n'
        printf '%s\n' "$pg_recipe"
        printf '\t@echo "test-integration OK: postgres suite passed"\n'
        printf '\n'
        printf 'test-leg: export CARGO_BUILD_JOBS := 8\n'
        printf '.PHONY: test-leg\n'
        printf 'test-leg:\n'
        printf '\t@echo "### workspace suite"\n'
        printf '%s\n' "$test_recipe"
        printf '\t@echo "test OK: workspace suite passed"\n'
    } >"$dir/Makefile"

    cat >"$dir/.dagger/main.go" <<EOF
package main

const (
	coreTestFeatures  = "postgres,wire-backend"
	testWorkspaceSkip = "-- --skip metadata::tests"
)

func (m *FraiseqlCi) integrationPostgres(ctx context.Context, source *dagger.Directory) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"echo '### integration: postgres'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-core --features 'postgres' --test '*'", // #880 canary
${pg_entries}
	}, "\\n")
	return nil, nil
}

func (m *FraiseqlCi) Test(ctx context.Context, source *dagger.Directory, rust string) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"echo '### cargo test --workspace (non-DB crates; wire+functions run separately)'",
${test_entries}
	}, "\\n")
	return nil, nil
}
EOF
}

# assert_gate <name> <expected-exit> <expected-substring> <dir>
assert_gate() {
    local name="$1" want_exit="$2" want_text="$3" dir="$4"
    TESTS_RUN=$((TESTS_RUN + 1))

    local out rc
    set +e
    out="$(python3 "$GATE" --root "$dir" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    # `-e` is load-bearing: half of what this gate reports starts with `--`, and
    # grep would read the expectation as its own option and exit 2 — a harness
    # bug that reads as the gate failing to report.
    if ! printf '%s' "$out" | grep -qF -e "$want_text"; then
        echo "  FAIL: $name — output did not contain: $want_text"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return
    fi
    echo "  ok: $name"
}

# ---------------------------------------------------------------------------
# The `Test` shard's default pair, kept MINIMAL on purpose.
#
# It first carried all three of this leg's distinctive shapes at once, and the
# suite looked thorough: reverting any one of the three parser behaviours turned
# it red. But it turned the SAME six assertions red every time — one broken shape
# breaks the whole pair, so every Test-side fixture fails together and not one of
# them pins the thing it names. Each shape gets its own fixture below instead.
# ---------------------------------------------------------------------------
TEST_ENTRIES_OK=$'\t\t"cargo test -p fraiseql-core --lib --features \'" + coreTestFeatures + "\'",'
TEST_RECIPE_OK=$'\tcargo test -p fraiseql-core --lib --features \'wire-backend,postgres\''

# The postgres shard's three gating lines. The first is assembled from a Go const,
# so the fixture also covers const resolution; without it the local spelling would
# look like a drift.
ENTRIES_OK=$'\t\t"cargo test -p fraiseql-core --features \'" + coreTestFeatures + ",test-postgres\' --test \'*\' -- --test-threads=1",\n\t\t"cargo test -p fraiseql-db --lib -- --test-threads=1",\n\t\t"echo \'### last\'",\n\t\t"cargo test -p fraiseql-cli --test doctor_against_db -- --test-threads=1",'

# The same three locally. Deliberately spelled differently in the two ways that
# are ALLOWED to differ: the feature list is in another order, and the line
# carries an env prefix the shard sets on the container instead.
RECIPE_OK=$'\t@cargo test -p fraiseql-core --features \'wire-backend,test-postgres,postgres\' --test \'*\' -- --test-threads=1\n\t@cargo test -p fraiseql-db --lib -- --test-threads=1\n\tDATABASE_URL="postgresql://u:p@localhost:5433/d" \\\n\t\tcargo test -p fraiseql-cli --test doctor_against_db -- --test-threads=1'

echo "=== tools/check-shard-parity.py ==="

# Both lists agree, on both shards. Feature-order, env-prefix and const-vs-literal
# spellings must all compare equal, or every real line would report as drift and
# the gate would be turned off within a week.
make_fixture "$WORK/ok" "$RECIPE_OK" "$ENTRIES_OK"
assert_gate "matching lists pass across the spellings that may differ" \
    0 "shard-parity: OK" "$WORK/ok"

# ---------------------------------------------------------------------------
# integrationPostgres ↔ make test-integration-postgres
# ---------------------------------------------------------------------------

# The #1169 shape itself: CI runs a suite the local mirror does not, so the
# target reports a green over tests it never ran.
RECIPE_MISSING=$'\t@cargo test -p fraiseql-core --features \'wire-backend,test-postgres,postgres\' --test \'*\' -- --test-threads=1\n\t@cargo test -p fraiseql-db --lib -- --test-threads=1'
make_fixture "$WORK/missing" "$RECIPE_MISSING" "$ENTRIES_OK"
assert_gate "a shard invocation absent from the local target is reported" \
    1 "doctor_against_db" "$WORK/missing"

# The other direction. A local-only line does not hide a failure, but it
# falsifies the target's claim to be the CI shard — which is the only reason to
# reach for it instead of plain cargo.
RECIPE_EXTRA="$RECIPE_OK"$'\n\t@cargo test -p fraiseql-cli --test invented_suite -- --test-threads=1'
make_fixture "$WORK/extra" "$RECIPE_EXTRA" "$ENTRIES_OK"
assert_gate "a local-only invocation is reported" \
    1 "invented_suite" "$WORK/extra"

# Flag drift, which is the specific thing that made these suites unrunnable
# locally. Same suite on both sides, different thread count: it must NOT compare
# equal, or the gate would bless the exact configuration that is red by
# construction.
RECIPE_THREADS=$'\t@cargo test -p fraiseql-core --features \'wire-backend,test-postgres,postgres\' --test \'*\' -- --test-threads=4\n\t@cargo test -p fraiseql-db --lib -- --test-threads=1\n\t@cargo test -p fraiseql-cli --test doctor_against_db -- --test-threads=1'
make_fixture "$WORK/threads" "$RECIPE_THREADS" "$ENTRIES_OK"
assert_gate "the same suite with a drifted --test-threads is reported" \
    1 "--test-threads=1" "$WORK/threads"

# A shard line in a shape the parser does not recognise. Dropping it silently
# would report a parity that was never checked over it.
ENTRIES_UNKNOWN="$ENTRIES_OK"$'\n\t\t"bash tools/some-new-gate.sh --against-db",'
make_fixture "$WORK/unknown" "$RECIPE_OK" "$ENTRIES_UNKNOWN"
assert_gate "an unclassifiable shard line is fatal, not dropped" \
    1 "cannot classify" "$WORK/unknown"

# A shard line built from an expression the const parser cannot resolve. Dropping
# the piece would leave `--features ''` — a well-formed cargo command carrying a
# silently wrong expectation, which is worse than not checking at all.
ENTRIES_UNRESOLVED="$ENTRIES_OK"$'\n\t\t"cargo test -p fraiseql-server --features \'" + computedAtRuntime + "\' --lib -- --test-threads=1",'
make_fixture "$WORK/unresolved" "$RECIPE_OK" "$ENTRIES_UNRESOLVED"
assert_gate "an unresolvable Go expression is reported, not silently emptied" \
    1 "<unresolved Go expression>" "$WORK/unresolved"

# ---------------------------------------------------------------------------
# Test ↔ make test-leg
# ---------------------------------------------------------------------------

# The #1257 shape: an invocation runs in CI and not locally.
TEST_ENTRIES_PLAIN="$TEST_ENTRIES_OK"$'\n\t\t"cargo test -p fraiseql-kafka",'
make_fixture "$WORK/test-missing" "$RECIPE_OK" "$ENTRIES_OK" "$TEST_RECIPE_OK" "$TEST_ENTRIES_PLAIN"
assert_gate "a Test-shard invocation absent from test-leg is reported" \
    1 "cargo test -p fraiseql-kafka" "$WORK/test-missing"

# The other direction, on this shard too.
TEST_RECIPE_EXTRA="$TEST_RECIPE_OK"$'\n\tcargo test -p fraiseql-invented --lib'
make_fixture "$WORK/test-extra" "$RECIPE_OK" "$ENTRIES_OK" "$TEST_RECIPE_EXTRA"
assert_gate "a test-leg-only invocation is reported" \
    1 "cargo test -p fraiseql-invented --lib" "$WORK/test-extra"

# An invocation written across several source lines with Go `+`. Read line by
# line it becomes six fragments, each unclassifiable — a loud failure, but for
# the wrong reason, and one that would push whoever met it toward exempting real
# lines. It must be read as ONE command, so the mirror's single line matches it.
ENTRIES_MULTILINE="$TEST_ENTRIES_OK"$'\n\t\t"cargo test --workspace" +\n\t\t\t" --exclude fraiseql-core" +\n\t\t\t" --all-features " + testWorkspaceSkip,'
RECIPE_MULTILINE="$TEST_RECIPE_OK"$'\n\tcargo test --workspace --exclude fraiseql-core --all-features -- --skip metadata::tests'
make_fixture "$WORK/multiline" "$RECIPE_OK" "$ENTRIES_OK" "$RECIPE_MULTILINE" "$ENTRIES_MULTILINE"
assert_gate "an invocation spanning Go + continuations is read as one command" \
    0 "shard-parity: OK" "$WORK/multiline"

# ...and the same shard line with the mirror's copy dropped, so the report names
# the whole joined command rather than a fragment of it.
make_fixture "$WORK/multiline-missing" "$RECIPE_OK" "$ENTRIES_OK" "$TEST_RECIPE_OK" "$ENTRIES_MULTILINE"
assert_gate "a dropped multi-line invocation is reported joined, not fragmented" \
    1 "cargo test --workspace --exclude fraiseql-core --all-features -- --skip metadata::tests" \
    "$WORK/multiline-missing"

# The `-- build` canary IS the shard's build step — no --no-run split, unlike the
# `-- test` form the postgres fixture carries, which the first assertion pins as
# exempt (the mirror runs that suite with different features and a thread cap, so
# a required canary line would report as drift there). A mirror that skips the
# build does not compile what CI compiles.
ENTRIES_CANARY_BUILD="$TEST_ENTRIES_OK"$'\n\t\t"bash tools/ci-target-canary.sh -- build --all-features",'
RECIPE_CANARY_BUILD="$TEST_RECIPE_OK"$'\n\tcargo build --all-features'
make_fixture "$WORK/canary-ok" "$RECIPE_OK" "$ENTRIES_OK" "$RECIPE_CANARY_BUILD" "$ENTRIES_CANARY_BUILD"
assert_gate "a canary-wrapped build matches the plain cargo build that mirrors it" \
    0 "shard-parity: OK" "$WORK/canary-ok"

make_fixture "$WORK/no-build" "$RECIPE_OK" "$ENTRIES_OK" "$TEST_RECIPE_OK" "$ENTRIES_CANARY_BUILD"
assert_gate "a canary-wrapped build the mirror omits is reported as the cargo build it is" \
    1 "cargo build --all-features" "$WORK/no-build"

# The mirror inventing a build the shard does not run, in the other direction.
TEST_RECIPE_EXTRA_BUILD="$TEST_RECIPE_OK"$'\n\tcargo build -p fraiseql-server --features invented'
make_fixture "$WORK/extra-build" "$RECIPE_OK" "$ENTRIES_OK" "$TEST_RECIPE_EXTRA_BUILD"
assert_gate "a local-only build is reported" \
    1 "cargo build -p fraiseql-server --features invented" "$WORK/extra-build"

# A `+` inside an argument must survive the concatenation split. Splitting on it
# would compare a command the shard never ran, and — unlike an unresolvable
# expression — the fragments still look like a valid invocation, so nothing would
# announce the substitution.
ENTRIES_PLUS_ARG="$TEST_ENTRIES_OK"$'\n\t\t"cargo test -p fraiseql-core --lib -- --skip legacy::a+b",'
RECIPE_PLUS_ARG="$TEST_RECIPE_OK"$'\n\tcargo test -p fraiseql-core --lib -- --skip legacy::a+b'
make_fixture "$WORK/plus-ok" "$RECIPE_OK" "$ENTRIES_OK" "$RECIPE_PLUS_ARG" "$ENTRIES_PLUS_ARG"
assert_gate "a + inside an argument compares equal to the same + locally" \
    0 "shard-parity: OK" "$WORK/plus-ok"

RECIPE_PLUS_SPLIT="$TEST_RECIPE_OK"$'\n\tcargo test -p fraiseql-core --lib -- --skip legacy::a b'
make_fixture "$WORK/plus-drift" "$RECIPE_OK" "$ENTRIES_OK" "$RECIPE_PLUS_SPLIT" "$ENTRIES_PLUS_ARG"
assert_gate "a + inside an argument is compared verbatim, not split" \
    1 "legacy::a+b" "$WORK/plus-drift"

# A whole-line comment among the recipe lines, at column 0 — make ignores it and
# keeps reading the recipe, so this gate must too. A TAB-indented `#` was never the
# problem (the shell gets it and the parser passes it through as an unclassifiable
# line); an unindented one is what ended the scan, and it is the spelling a Makefile
# comment naturally takes.
RECIPE_COMMENTED=$'# why the next line is here\n'"$TEST_RECIPE_OK"
make_fixture "$WORK/commented" "$RECIPE_OK" "$ENTRIES_OK" "$RECIPE_COMMENTED"
assert_gate "a comment among the recipe lines does not truncate the recipe" \
    0 "shard-parity: OK" "$WORK/commented"

# A `+` in an echo's prose is prose. Three of the real shard's echo lines carry
# one, and reading them as concatenation left two fragments that were neither
# quoted nor a const — so the gate called a plain echo an unreadable command.
ENTRIES_PLUS_PROSE="$TEST_ENTRIES_OK"$'\n\t\t"echo \'### config manifest + doc examples\'",'
make_fixture "$WORK/prose" "$RECIPE_OK" "$ENTRIES_OK" "$TEST_RECIPE_OK" "$ENTRIES_PLUS_PROSE"
assert_gate "a + in an echo's prose does not make it unclassifiable" \
    0 "shard-parity: OK" "$WORK/prose"

# ---------------------------------------------------------------------------
# Shapes that make the gate blind rather than wrong
# ---------------------------------------------------------------------------

# If a Go literal is reshaped so the parser extracts nothing, the gate must fail
# loudly rather than pass vacuously over an empty requirement set.
mkdir -p "$WORK/blind/.dagger"
cp "$WORK/ok/Makefile" "$WORK/blind/Makefile"
cat >"$WORK/blind/.dagger/main.go" <<'EOF'
package main

func (m *FraiseqlCi) integrationPostgres(ctx context.Context, source *dagger.Directory) (string, error) {
	script := strings.Join([]string{
		"set -e",
	}, "\n")
	return nil, nil
}

func (m *FraiseqlCi) Test(ctx context.Context, source *dagger.Directory, rust string) (string, error) {
	script := strings.Join([]string{
		"set -e",
	}, "\n")
	return nil, nil
}
EOF
assert_gate "an empty shard literal fails loudly, not vacuously" \
    1 "went blind" "$WORK/blind"

# A mirror target deleted (or renamed) while its shard keeps running.
mkdir -p "$WORK/notarget/.dagger"
cp "$WORK/ok/.dagger/main.go" "$WORK/notarget/.dagger/main.go"
printf 'test-unit:\n\t@cargo test --lib\n' >"$WORK/notarget/Makefile"
assert_gate "a missing mirror target is reported" \
    1 "no \`test-integration-postgres\` target" "$WORK/notarget"
assert_gate "the second missing mirror target is reported too" \
    1 "no \`test-leg\` target" "$WORK/notarget"

# A shard function renamed away from what SHARDS names.
mkdir -p "$WORK/noshard/.dagger"
cp "$WORK/ok/Makefile" "$WORK/noshard/Makefile"
printf 'package main\n\nfunc (m *FraiseqlCi) integrationSomethingElse() {}\n' \
    >"$WORK/noshard/.dagger/main.go"
assert_gate "a renamed shard function is reported" \
    1 "cannot read the \`integrationPostgres\` shard" "$WORK/noshard"
assert_gate "a renamed Test shard is reported" \
    1 "cannot read the \`Test\` shard" "$WORK/noshard"

echo ""
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "PASSED: $TESTS_RUN assertions"
