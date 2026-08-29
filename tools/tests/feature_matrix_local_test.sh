#!/usr/bin/env bash
# Red-capability pin for tools/feature-combos.py + tools/lint-feature-matrix.sh.
#
# Run directly:  bash tools/tests/feature_matrix_local_test.sh
# Exits non-zero if any assertion fails.
#
# The local matrix runner exists (#1227) because no local gate ran clippy under a
# narrow feature set, so a nursery lint on a feature-OFF build reached `dev`. Its one
# dangerous failure mode is the one it was built to prevent in the first place:
# running FEWER combos than `.dagger/feature-combos.go` declares and still printing a
# ✅. So the assertions below are mostly about that — a literal the parser cannot read,
# a field it does not model, and a `cargoArgs()` it no longer mirrors must each be
# FATAL, never a shorter list.
#
# `cargo` is stubbed for the runner assertions: this pin checks the runner's plumbing
# (does it invoke every discovered combo, with the leg's exact argv, and does a failure
# propagate), not whether the workspace compiles. Compiling is what the gate itself
# does, and it takes ~25 minutes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A fixture repo carrying only what the tools read. `git init` because both tools
# resolve the root with `git rev-parse --show-toplevel`.
make_fixture() {
    local dir="$1"
    mkdir -p "$dir/tools/tests" "$dir/.dagger" "$dir/bin"
    cp "$REPO_ROOT/tools/feature-combos.py" "$dir/tools/"
    cp "$REPO_ROOT/tools/lint-feature-matrix.sh" "$dir/tools/"
    cp "$REPO_ROOT/.dagger/feature-combos.go" "$dir/.dagger/"
    git -C "$dir" init -q .
    # A `cargo` that records its argv and succeeds. Individual cases override it.
    cat >"$dir/bin/cargo" <<'STUB'
#!/usr/bin/env bash
printf 'cargo %s\n' "$*" >> "$CARGO_STUB_LOG"
exit 0
STUB
    chmod +x "$dir/bin/cargo"
    # ShellGates has no Rust toolchain by design, and this pin runs there. The runner
    # prints `rustc --version` for the record; stub it so the fixture is hermetic.
    printf '#!/usr/bin/env bash\necho "rustc 0.0.0 (stub)"\n' >"$dir/bin/rustc"
    chmod +x "$dir/bin/rustc"
}

# expect_parser <pass|fail> <case-id> <description> <mutation...>
expect_parser() {
    local want="$1" id="$2" desc="$3"; shift 3
    local dir="$WORK/$id" got
    TESTS_RUN=$((TESTS_RUN + 1))
    make_fixture "$dir"
    ( cd "$dir" && "$@" ) || { printf '  ❌ %-4s mutation itself failed\n' "$id"; TESTS_FAILED=$((TESTS_FAILED+1)); return; }

    if ( cd "$dir" && python3 tools/feature-combos.py >"$dir/.out" 2>&1 ); then got=pass; else got=fail; fi

    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$desc" "$want" "$got"
        head -20 "$dir/.out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# assert <case-id> <description> <expected-exit> <command...>  (run inside the fixture)
run_case() {
    local id="$1" want_rc="$2"; shift 2
    local dir="$WORK/$id" rc=0
    make_fixture "$dir"
    export CARGO_STUB_LOG="$dir/.cargo-log"
    : >"$CARGO_STUB_LOG"
    ( cd "$dir" && PATH="$dir/bin:$PATH" "$@" >"$dir/.out" 2>&1 ) || rc=$?
    CASE_DIR="$dir"; CASE_RC="$rc"; CASE_WANT="$want_rc"
}

check() {
    local id="$1" desc="$2" ok="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    if [ "$ok" = "yes" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s\n' "$id" "$desc"
        head -20 "$CASE_DIR/.out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

noop() { :; }

# Replace the first line matching $1 with $2, in .dagger/feature-combos.go.
sub_line() {
    python3 - "$1" "$2" <<'PY'
import sys, pathlib
needle, replacement = sys.argv[1], sys.argv[2]
p = pathlib.Path(".dagger/feature-combos.go")
s = p.read_text()
assert needle in s, f"fixture mutation needle not found: {needle}"
p.write_text(s.replace(needle, replacement, 1))
PY
}

echo "feature-matrix local-runner self-test"
echo

echo "── the parser reads the tree as it stands (or every assertion below is vacuous) ──"
expect_parser pass P0 "repository as it stands" noop

echo
echo "── it can never yield a SHORTER list than the file declares ──"
# A literal split across lines: the shape a `gofmt` reflow or a long comment would
# produce. Silently skipping it is the whole defect class.
expect_parser fail S1 "a multi-line combo literal is fatal, not skipped" \
    sub_line '{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},' \
             '{name: "server-gcs", crate: "fraiseql-server",
		features: []string{"gcs"}},'
# A field the parser does not model changes the leg's invocation; dropping it makes
# the local run stop matching CI without saying so.
expect_parser fail S2 "an unmodelled featureCombo field is fatal" \
    sub_line '{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},' \
             '{name: "server-gcs", crate: "fraiseql-server", toolchain: "nightly", features: []string{"gcs"}},'
expect_parser fail S3 "a duplicate combo name is fatal" \
    sub_line '{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},' \
             '{name: "server-secrets", crate: "fraiseql-server", features: []string{"gcs"}},'
expect_parser fail S4 "a renamed featureCombos var is fatal" \
    sub_line 'var featureCombos = []featureCombo{' 'var featureCombosV2 = []featureCombo{'

echo
echo "── it stops mirroring cargoArgs() the moment cargoArgs() moves ──"
expect_parser fail H1 "a changed cargoArgs() body trips the hash pin" \
    sub_line 'args := []string{"cargo", sub, "-p", c.crate}' \
             'args := []string{"cargo", sub, "--locked", "-p", c.crate}'
expect_parser fail H2 "a deleted cargoArgs() is fatal" \
    sub_line 'func (c featureCombo) cargoArgs() []string {' \
             'func (c featureCombo) cargoArgsRenamed() []string {'

echo
echo "── a combo added to the Go data reaches the runner ──"
TESTS_RUN=$((TESTS_RUN + 1))
dir="$WORK/A1"; make_fixture "$dir"
( cd "$dir" && sub_line '{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},' \
    '{name: "server-gcs", crate: "fraiseql-server", features: []string{"gcs"}},
	{name: "brand-new-combo", crate: "fraiseql-core", noDefaultFeatures: true, clippy: true, features: []string{"postgres", "x"}},' )
if ( cd "$dir" && python3 tools/feature-combos.py 2>&1 | grep -qxF \
        'brand-new-combo	cargo clippy -p fraiseql-core --no-default-features --features postgres,x --all-targets -- -D warnings' ); then
    printf '  ✅ %-4s a new combo appears with the leg'"'"'s exact argv\n' A1
else
    printf '  ❌ %-4s a new combo appears with the leg'"'"'s exact argv\n' A1
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo
echo "── the runner invokes every combo it discovered, and only those ──"
run_case R1 0 bash tools/lint-feature-matrix.sh
declared="$( cd "$WORK/R1" && python3 tools/feature-combos.py | wc -l )"
invoked="$( wc -l < "$WORK/R1/.cargo-log" )"
check R1 "invoked == declared ($invoked == $declared), exit 0" \
    "$( [ "$CASE_RC" -eq 0 ] && [ "$invoked" -eq "$declared" ] && echo yes || echo no )"

TESTS_RUN=$((TESTS_RUN + 1))
if diff -q <( cd "$WORK/R1" && python3 tools/feature-combos.py | cut -f2 ) "$WORK/R1/.cargo-log" >/dev/null; then
    printf '  ✅ %-4s each invocation is byte-identical to the derived argv, in order\n' R2
else
    printf '  ❌ %-4s each invocation is byte-identical to the derived argv, in order\n' R2
    diff <( cd "$WORK/R1" && python3 tools/feature-combos.py | cut -f2 ) "$WORK/R1/.cargo-log" > "$WORK/R1/.diff" || true
    head -10 "$WORK/R1/.diff" | sed 's/^/        /'
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo
echo "── a failing combo reddens the run and is named ──"
dir="$WORK/R3"; make_fixture "$dir"
cat >"$dir/bin/cargo" <<'STUB'
#!/usr/bin/env bash
printf 'cargo %s\n' "$*" >> "$CARGO_STUB_LOG"
case "$*" in *admin-sql*) echo "error: collection is never read"; exit 101 ;; esac
exit 0
STUB
chmod +x "$dir/bin/cargo"
export CARGO_STUB_LOG="$dir/.cargo-log"; : >"$CARGO_STUB_LOG"
rc=0
( cd "$dir" && PATH="$dir/bin:$PATH" bash tools/lint-feature-matrix.sh >"$dir/.out" 2>&1 ) || rc=$?
CASE_DIR="$dir"
check R3 "one bad combo ⇒ exit 1 (got $rc), named in the summary" \
    "$( [ "$rc" -eq 1 ] && grep -q 'server-admin-sql' "$dir/.out" && grep -q '❌' "$dir/.out" && echo yes || echo no )"
# fail-fast is OFF, matching the leg: the run must continue past the failure.
after="$( grep -c . "$dir/.cargo-log" )"
check R4 "fail-fast is off — all $after combos still ran" \
    "$( [ "$after" -eq "$declared" ] && echo yes || echo no )"

echo
echo "── a narrowed run says so, and never claims the whole matrix ──"
run_case R5 0 bash tools/lint-feature-matrix.sh --clippy-only
check R5 "--clippy-only prints NARROWED and a subset count" \
    "$( [ "$CASE_RC" -eq 0 ] && grep -q 'NARROWED' "$CASE_DIR/.out" && grep -q 'narrowed run' "$CASE_DIR/.out" && echo yes || echo no )"
run_case R6 2 bash tools/lint-feature-matrix.sh --combo=does-not-exist
check R6 "an unknown --combo= exits 2 rather than running nothing and passing" \
    "$( [ "$CASE_RC" -eq 2 ] && echo yes || echo no )"

echo
echo "── a broken combo file can never produce a green run ──"
dir="$WORK/R7"; make_fixture "$dir"
( cd "$dir" && sub_line 'var featureCombos = []featureCombo{' 'var featureCombosV2 = []featureCombo{' )
export CARGO_STUB_LOG="$dir/.cargo-log"; : >"$CARGO_STUB_LOG"
rc=0
( cd "$dir" && PATH="$dir/bin:$PATH" bash tools/lint-feature-matrix.sh >"$dir/.out" 2>&1 ) || rc=$?
CASE_DIR="$dir"
check R7 "an unparseable feature-combos.go ⇒ exit 2, zero cargo invocations" \
    "$( [ "$rc" -eq 2 ] && [ ! -s "$dir/.cargo-log" ] && echo yes || echo no )"

echo
if [ "$TESTS_FAILED" -eq 0 ]; then
    echo "✅ feature-matrix local-runner self-test: $TESTS_RUN assertions passed"
else
    echo "❌ feature-matrix local-runner self-test: $TESTS_FAILED of $TESTS_RUN assertions FAILED"
fi
exit $((TESTS_FAILED > 0))
