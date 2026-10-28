#!/usr/bin/env bash
# Unit tests for tools/bench-require-measured.py.
#
# Run directly:  bash tools/tests/bench_require_measured_test.sh
# Exits non-zero if any assertion fails.
#
# A velocitybench run that measured nothing is a broken harness, not a quiet result:
# every row skipped (the shape of a run whose service never came up) or every request
# failed must fail the job. A query velocitybench skips for one framework does not,
# as long as another was measured.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECK="$REPO_ROOT/tools/bench-require-measured.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# row <query> <skipped> <requests> <errors> [skip_reason]
row() {
    local rps=0
    [ "$2" = false ] && [ "$3" -gt "$4" ] && rps=1234.5
    printf '{"framework":"fraiseql-tv","query":"%s","pass":1,"rps":%s,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"requests":%s,"errors":%s,"error_breakdown":{},"skipped":%s,"skip_reason":"%s"}' \
        "$1" "$rps" "$3" "$4" "$2" "${5:-}"
}
# results <file> <row>...
results() {
    local file="$1" sep="" rows=""
    shift
    for r in "$@"; do rows="$rows$sep$r"; sep=","; done
    printf '{"environment":{},"framework_versions":{},"results":[%s],"db_footprint":{}}\n' "$rows" > "$file"
}

# check <name> <expected-rc> <expected-substring> <results-file>
check() {
    local name="$1" want_rc="$2" want="$3" file="$4" rc=0
    python3 -I "$CHECK" "$file" > "$WORK/$name.out" 2>&1 || rc=$?
    TESTS_RUN=$((TESTS_RUN + 1))
    if [ "$rc" -ne "$want_rc" ] || ! grep -qF -- "$want" "$WORK/$name.out"; then
        echo "FAIL [$name]: rc $rc (want $want_rc), '$want' in output?"
        sed 's/^/  | /' "$WORK/$name.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "ok   [$name]"
    fi
}

# 1. The shape of a run whose service never came up: 16 queries, all skipped.
rows=()
for q in Q1 Q2 Q2b Q3 Q4 Q5 Q6 Q7 Q8 Q9 Q10 Q11 Q12 M1 MC1 MC2; do
    rows+=("$(row "$q" true 0 0 'service did not become healthy')")
done
results "$WORK/all-skipped.json" "${rows[@]}"
check all-skipped 1 'service did not become healthy' "$WORK/all-skipped.json"

# 2. Every request failed: nothing was measured either.
results "$WORK/all-errors.json" "$(row Q1 false 500 500)" "$(row Q2 false 300 300)"
check all-errors 1 'nothing was measured' "$WORK/all-errors.json"

# 3. No rows at all.
results "$WORK/empty.json"
check empty 1 'nothing was measured' "$WORK/empty.json"

# 4. One query measured, one skipped for this framework: that is a result.
results "$WORK/some.json" "$(row Q1 false 9000 0)" "$(row MC1 true 0 0 skipped)"
check some-measured 0 '1 of 2' "$WORK/some.json"

# 5. A measured query with a few failed requests still measured something.
results "$WORK/few-errors.json" "$(row Q1 false 9000 3)"
check few-errors 0 '1 of 1' "$WORK/few-errors.json"

# 6. A row the harness marked skipped is not a measurement, whatever counts it carries
#    (velocitybench writes none today; a partial run recorded as skipped must not pass).
results "$WORK/skipped-counts.json" "$(row Q1 true 9000 0 'aborted')"
check skipped-with-counts 1 '1 × aborted' "$WORK/skipped-counts.json"

# 7. A results file that is missing or not JSON is a broken harness.
check missing 2 'cannot read' "$WORK/nope.json"
echo 'not json' > "$WORK/garbage.json"
check garbage 2 'cannot read' "$WORK/garbage.json"

# 8. The release workflow runs this check only on a run that measured: "skipped: not
#    quiet" writes no results file, and must stay green. Read the step's own block, so a
#    condition on a neighbouring step cannot satisfy it.
WORKFLOW="$REPO_ROOT/.github/workflows/benchmark-velocity.yml"
TESTS_RUN=$((TESTS_RUN + 1))
step="$(awk '/^      - name: /{inside = ($0 ~ /Require a measurement$/)} inside' "$WORKFLOW")"
if ! grep -qF 'bench-require-measured.py' <<< "$step" \
    || ! grep -qxF "        if: steps.quiet.outputs.measure == 'true'" <<< "$step"; then
    echo "FAIL [workflow-skips-when-not-quiet]: the 'Require a measurement' step must run"
    echo "  bench-require-measured.py under: if: steps.quiet.outputs.measure == 'true'"
    sed 's/^/  | /' <<< "$step"
    TESTS_FAILED=$((TESTS_FAILED + 1))
else
    echo "ok   [workflow-skips-when-not-quiet]"
fi

echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
