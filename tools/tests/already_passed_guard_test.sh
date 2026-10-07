#!/usr/bin/env bash
# Unit tests for tools/check-already-passed-guard.py.
#
# Run directly:  bash tools/tests/already_passed_guard_test.sh
# Exits non-zero if any assertion fails.
#
# Every refusal branch gets a fixture that only THAT branch rejects, and the clean
# shapes get fixtures that must pass, so a gate that rejects everything (or
# nothing) fails here.
#
# Exit codes of the gate: 0 = clean, 1 = findings, 2 = FATAL.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-already-passed-guard.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# fixture <name> <file> <yaml> → prints the root dir to scan. Every fixture also
# holds the exempt workflow, so the EXEMPT table never names a missing file.
fixture() {
    local dir="$WORK/$1"
    mkdir -p "$dir/.github/workflows"
    printf '%s' "$3" >"$dir/.github/workflows/$2"
    cat >"$dir/.github/workflows/dagger-security.yml" <<'YML'
on:
  push:
jobs:
  security:
    runs-on: [self-hosted]
    steps:
      - run: true
YML
    echo "$dir"
}

# expect <label> <want-exit> <want-substring> <root>
expect() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local out code=0
    out="$(ALREADY_PASSED_ROOT="$4" python3 "$GATE" 2>&1)" || code=$?
    if [ "$code" != "$2" ] || ! grep -qF -- "$3" <<<"$out"; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        echo "FAIL  $1 (exit $code, wanted $2 and \"$3\")"
        printf '%s\n' "$out" | sed 's/^/        /'
    else
        echo "PASS  $1"
    fi
}

GUARDED='on:
  push:
jobs:
  already-passed:
    uses: ./.github/workflows/already-passed.yml
    with:
      workflow: leg.yml
  leg:
    needs: already-passed
    if: ${{ !cancelled() && needs.already-passed.outputs.passed != '"'true'"' }}
    runs-on: [self-hosted, archbox]
    steps:
      - run: true
'

expect "a guarded self-hosted push job is clean" 0 "1 self-hosted push jobs guarded" \
    "$(fixture clean leg.yml "$GUARDED")"

# A: no guard at all — the dagger-image.yml shape before the fix.
expect "an unguarded self-hosted push job is refused" 1 "does not \`needs:\`" \
    "$(fixture unguarded leg.yml 'on:
  push:
    branches: [dev]
jobs:
  leg:
    runs-on: [self-hosted, archbox]
    steps:
      - run: true
')"

# B: the guard asks about another workflow (a copy-pasted leg).
expect "a guard asking about another workflow is refused" 1 "asks about \`other.yml\`" \
    "$(fixture wrongfile leg.yml "${GUARDED/workflow: leg.yml/workflow: other.yml}")"

# C: needs the guard but ignores its answer.
expect "a job that ignores the guard's output is refused" 1 "\`if:\` must be" \
    "$(fixture noif leg.yml "$(grep -v '^    if:' <<<"$GUARDED")
")"

# C: reads the answer but would be skipped when the guard job itself fails.
expect "a job dropped when the guard fails is refused" 1 "\`if:\` must be" \
    "$(fixture nocancelled leg.yml "${GUARDED/!cancelled() && /}")"

# Not in scope: GitHub-hosted jobs, and workflows no push can start.
expect "a GitHub-hosted push job needs no guard" 0 "already-passed guard: OK" \
    "$(fixture hosted leg.yml "$GUARDED" && cat >"$WORK/hosted/.github/workflows/lint.yml" <<'YML'
on:
  push:
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - run: true
YML
)"
expect "a dispatch-only self-hosted job needs no guard" 0 "already-passed guard: OK" \
    "$(fixture dispatch leg.yml "$GUARDED" && cat >"$WORK/dispatch/.github/workflows/bench.yml" <<'YML'
on:
  workflow_dispatch:
jobs:
  bench:
    runs-on: [self-hosted]
    steps:
      - run: true
YML
)"

# A scan that guards nothing proves nothing.
expect "an empty scan is FATAL" 2 "scanned nothing" \
    "$(fixture empty lint.yml 'on:
  push:
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - run: true
')"

# The exemption must name a file that exists.
dir="$(fixture stale leg.yml "$GUARDED")"
rm "$dir/.github/workflows/dagger-security.yml"
expect "an EXEMPT row naming a missing workflow is refused" 1 "does not exist" "$dir"

# The live tree.
expect "the repository's own workflows are clean" 0 "already-passed guard: OK" "$REPO_ROOT"

echo
echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
