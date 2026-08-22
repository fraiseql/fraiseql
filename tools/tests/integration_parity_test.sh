#!/usr/bin/env bash
# Unit tests for tools/check-integration-parity.py.
#
# Run directly:  bash tools/tests/integration_parity_test.sh
# Exits non-zero if any assertion fails.
#
# The gate exists because `make test-integration-postgres` is a hand-maintained
# copy of the line list in `.dagger/main.go`'s integrationPostgres, and the whole
# point of that target is to be trusted as "what CI runs" (#1169). A parity gate
# that cannot itself go red would just be a third place the same false assurance
# lives, so every way the two lists can diverge has a fixture below and every one
# must be reported — including the two that make a gate worth *less* than no
# gate: a shard line the parser cannot classify, and a shard literal it reads as
# empty.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-integration-parity.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# make_fixture <dir> <makefile-recipe-lines> <shellgates-entries>
# Builds a minimal tree with the same shapes the real files use: target-specific
# variable lines before the rule, `@`-prefixed recipe lines, and a Go script
# literal whose feature lists are assembled from a const.
make_fixture() {
    local dir="$1" recipe="$2" entries="$3"
    mkdir -p "$dir/.dagger"

    {
        printf 'test-integration-postgres: export DATABASE_URL := postgresql://u:p@localhost:5433/d\n'
        printf 'test-integration-postgres: export RUST_LOG := debug\n'
        printf '.PHONY: test-integration-postgres\n'
        printf 'test-integration-postgres: db-up db-failover-reset\n'
        printf '\t@echo "### core sweep"\n'
        printf '%s\n' "$recipe"
        printf '\t@echo "test-integration OK: postgres suite passed"\n'
    } >"$dir/Makefile"

    cat >"$dir/.dagger/main.go" <<EOF
package main

const (
	coreTestFeatures = "postgres,wire-backend"
)

func (m *FraiseqlCi) integrationPostgres(ctx context.Context, source *dagger.Directory) (string, error) {
	script := strings.Join([]string{
		"set -e",
		"echo '### integration: postgres'",
		"bash tools/ci-target-canary.sh -- test -p fraiseql-core --features 'postgres' --test '*'", // #880 canary
${entries}
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

# The shard's three gating lines. The first is assembled from a Go const, so the
# fixture also covers const resolution; without it the local spelling would look
# like a drift.
ENTRIES_OK=$'\t\t"cargo test -p fraiseql-core --features \'" + coreTestFeatures + ",test-postgres\' --test \'*\' -- --test-threads=1",\n\t\t"cargo test -p fraiseql-db --lib -- --test-threads=1",\n\t\t"echo \'### last\'",\n\t\t"cargo test -p fraiseql-cli --test doctor_against_db -- --test-threads=1",'

# The same three locally. Deliberately spelled differently in the two ways that
# are ALLOWED to differ: the feature list is in another order, and the line
# carries an env prefix the shard sets on the container instead.
RECIPE_OK=$'\t@cargo test -p fraiseql-core --features \'wire-backend,test-postgres,postgres\' --test \'*\' -- --test-threads=1\n\t@cargo test -p fraiseql-db --lib -- --test-threads=1\n\tDATABASE_URL="postgresql://u:p@localhost:5433/d" \\\n\t\tcargo test -p fraiseql-cli --test doctor_against_db -- --test-threads=1'

echo "=== tools/check-integration-parity.py ==="

# Both lists agree. Feature-order, env-prefix and const-vs-literal spellings must
# all compare equal, or every real line would report as drift and the gate would
# be turned off within a week.
make_fixture "$WORK/ok" "$RECIPE_OK" "$ENTRIES_OK"
assert_gate "matching lists pass across the spellings that may differ" \
    0 "integration-parity: OK" "$WORK/ok"

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

# If the Go literal is reshaped so the parser extracts nothing, the gate must
# fail loudly rather than pass vacuously over an empty requirement set.
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
EOF
assert_gate "an empty shard literal fails loudly, not vacuously" \
    1 "went blind" "$WORK/blind"

# The mirror target deleted (or renamed) while the shard keeps running.
mkdir -p "$WORK/notarget/.dagger"
cp "$WORK/ok/.dagger/main.go" "$WORK/notarget/.dagger/main.go"
printf 'test-unit:\n\t@cargo test --lib\n' >"$WORK/notarget/Makefile"
assert_gate "a missing mirror target is reported" \
    1 "no \`test-integration-postgres\` target" "$WORK/notarget"

# The shard function renamed away from what SHARDS names.
mkdir -p "$WORK/noshard/.dagger"
cp "$WORK/ok/Makefile" "$WORK/noshard/Makefile"
printf 'package main\n\nfunc (m *FraiseqlCi) integrationSomethingElse() {}\n' \
    >"$WORK/noshard/.dagger/main.go"
assert_gate "a renamed shard function is reported" \
    1 "cannot read the \`integrationPostgres\` shard" "$WORK/noshard"

echo ""
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "PASSED: $TESTS_RUN assertions"
