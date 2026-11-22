#!/usr/bin/env bash
# Unit tests for tools/check-fuzz-targets.sh.
#
# Run directly:  bash tools/tests/fuzz_targets_test.sh
# Exits non-zero if any assertion fails.
#
# Each case builds a small tree (crates/<c>/fuzz/{Cargo.toml,fuzz_targets/}) and a
# workflow, and checks the gate's verdict. Most are shapes the gate must REJECT.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$SCRIPT_DIR/../check-fuzz-targets.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

TESTS_RUN=0
TESTS_FAILED=0

# target <dir> <crate> <name>: a fuzz target on disk, with its [[bin]].
target() {
    mkdir -p "$1/crates/$2/fuzz/fuzz_targets"
    printf '[[bin]]\nname = "%s"\n' "$3" >>"$1/crates/$2/fuzz/Cargo.toml"
    printf '#![no_main]\n' >"$1/crates/$2/fuzz/fuzz_targets/$3.rs"
}

# workflow <dir> <timeout-minutes> <dispatch-budget> <schedule-budget> <rows...>
workflow() {
    local dir="$1" timeout="$2" dispatch="$3" schedule="$4"
    shift 4
    {
        printf 'on:\n  workflow_dispatch:\n    inputs:\n      max_total_time:\n'
        printf '        default: "%s"\n' "$dispatch"
        printf 'env:\n'
        # shellcheck disable=SC2016  # the workflow expression is literal text
        printf '  FUZZ_MAX_TOTAL_TIME: ${{ github.event.inputs.max_total_time || '"'"'%s'"'"' }}\n' "$schedule"
        printf 'jobs:\n  fuzz:\n    timeout-minutes: %s\n    strategy:\n      matrix:\n        include:\n' "$timeout"
        local row
        for row in "$@"; do
            printf '          - { crate: %s, target: %s }\n' "${row%/*}" "${row#*/}"
        done
    } >"$dir/fuzz.yml"
}

# check <name> <dir> <expected exit> [<text the output must hold>]
check() {
    local name="$1" dir="$2" want="$3" text="${4:-}" out rc
    TESTS_RUN=$((TESTS_RUN + 1))
    set +e
    out="$(FUZZ_TARGETS_ROOT="$dir" FUZZ_TARGETS_WORKFLOW="$dir/fuzz.yml" \
        FUZZ_UNSCHEDULED="$dir/unscheduled.txt" bash "$GATE" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want" ] || { [ -n "$text" ] && ! grep -F -- "$text" <<<"$out" >/dev/null; }; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        echo "FAIL: $name: exit $rc, want $want${text:+, with \"$text\"}"
        sed 's/^/    /' <<<"$out"
    fi
}

new() {
    local dir="$WORK/$1"
    mkdir -p "$dir"
    : >"$dir/unscheduled.txt"
    echo "$dir"
}

# --- accepted -----------------------------------------------------------------------

d="$(new all_scheduled)"
target "$d" a one
target "$d" b two
workflow "$d" 60 300 900 a/one b/two
check all_scheduled "$d" 0

d="$(new exempted_with_a_reason)"
target "$d" a one
target "$d" a two
workflow "$d" 60 300 900 a/one
echo 'a/two  needs a database to reach its parser' >"$d/unscheduled.txt"
check exempted_with_a_reason "$d" 0

# --- refused --------------------------------------------------------------------------

d="$(new a_target_no_row_runs)"
target "$d" a one
target "$d" a two
workflow "$d" 60 300 900 a/one
check a_target_no_row_runs "$d" 1 "a / two — a fuzz target on disk"

d="$(new an_exemption_without_a_reason)"
target "$d" a one
target "$d" a two
workflow "$d" 60 300 900 a/one
echo 'a/two' >"$d/unscheduled.txt"
check an_exemption_without_a_reason "$d" 1 "has no reason"

d="$(new an_exemption_for_a_scheduled_target)"
target "$d" a one
workflow "$d" 60 300 900 a/one
echo 'a/one  stale' >"$d/unscheduled.txt"
check an_exemption_for_a_scheduled_target "$d" 1 "is scheduled in"

d="$(new an_exemption_for_a_missing_target)"
target "$d" a one
workflow "$d" 60 300 900 a/one
echo 'a/gone  deleted last month' >"$d/unscheduled.txt"
check an_exemption_for_a_missing_target "$d" 1 "is not a fuzz target on disk"

d="$(new a_row_for_a_missing_target)"
target "$d" a one
workflow "$d" 60 300 900 a/one a/two
check a_row_for_a_missing_target "$d" 1 "no [[bin]] named \"two\""

d="$(new a_row_naming_the_wrong_crate)"
target "$d" a one
target "$d" b two
workflow "$d" 60 300 900 a/one a/two b/two
check a_row_naming_the_wrong_crate "$d" 1 "a target with that name exists in"

d="$(new a_budget_past_the_timeout)"
target "$d" a one
workflow "$d" 30 300 1800 a/one
check a_budget_past_the_timeout "$d" 1 "cannot finish inside the job timeout"

d="$(new no_rows_parsed)"
target "$d" a one
workflow "$d" 60 300 900
check no_rows_parsed "$d" 1 "no matrix rows parsed"

echo "fuzz targets gate: $TESTS_RUN cases, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
