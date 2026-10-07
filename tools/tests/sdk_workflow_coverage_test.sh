#!/usr/bin/env bash
# Unit tests for tools/check-sdk-workflow-coverage.py (#1119, #1467) and for
# tools/sdk-suites-matrix.py, which decides what that workflow runs.
#
# Run directly:  bash tools/tests/sdk_workflow_coverage_test.sh
# Exits non-zero if any assertion fails.
#
# One valid fixture tree, then one mutation per rule, each refused by that rule alone,
# so a gate that refuses everything or nothing fails here.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-sdk-workflow-coverage.py"
MATRIX="$REPO_ROOT/tools/sdk-suites-matrix.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass() { TESTS_RUN=$((TESTS_RUN + 1)); echo "PASS  $1"; }
fail() {
    TESTS_RUN=$((TESTS_RUN + 1)); TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "FAIL  $1"
    [ -n "${2:-}" ] && printf '%s\n' "$2" | sed 's/^/        /'
    return 0
}

# fixture <name> → a tree the gate accepts: one SDK (`demo`), its matrix row, an
# unfiltered workflow with a setup step and the suite, the aggregate, the required line.
fixture() {
    local dir="$WORK/$1"
    mkdir -p "$dir/tools" "$dir/sdks/official/fraiseql-demo" "$dir/sdks/official/conformance" \
             "$dir/.github/workflows"
    printf 'VERSIONS = {"demo": ["1"]}\n' >"$dir/tools/sdk-suites-matrix.py"
    printf 'required = [\n  "SDK suites",\n]\n' >"$dir/tools/required-checks.toml"
    cat >"$dir/.github/workflows/sdk-suites.yml" <<'YML'
name: SDK suites
on:
  push:
    branches-ignore: ['dependabot/**']
jobs:
  changes:
    runs-on: ubuntu-latest
    steps:
      - run: python3 tools/sdk-suites-matrix.py
  suite:
    needs: changes
    runs-on: ubuntu-latest
    steps:
      - if: ${{ matrix.sdk == 'demo' }}
        uses: actions/setup-demo@v1
      - run: bash tools/sdk-suite.sh "${{ matrix.sdk }}"
  aggregate:
    name: SDK suites
    needs: [changes, suite]
    if: ${{ always() }}
    runs-on: ubuntu-latest
    steps:
      - run: true
YML
    echo "$dir"
}

# expect <label> <want-exit> <needle> <root>
expect() {
    local rc=0 out
    out="$(SDK_WORKFLOW_ROOT="$4" python3 "$GATE" 2>&1)" || rc=$?
    if [ "$rc" -ne "$2" ]; then fail "$1: exit $rc, wanted $2" "$out"; return; fi
    if ! grep -qF -- "$3" <<<"$out"; then fail "$1: output lacks '$3'" "$out"; return; fi
    pass "$1"
}

# edit <dir> <file> <python-expr over s> — rewrite one fixture file in place.
edit() {
    python3 - "$1/$2" "$3" <<'PY'
import sys
p, expr = sys.argv[1], sys.argv[2]
s = open(p).read()
new = eval(expr, {"s": s})
assert new != s, f"mutation did not change {p}"
open(p, "w").write(new)
PY
}

echo "── the gate ──"

expect "the valid fixture passes" 0 "all 1 official SDKs" "$(fixture ok)"

# A. filtered, or not every branch — the #1467 shape and the #1119 shapes.
d="$(fixture paths)"
edit "$d" .github/workflows/sdk-suites.yml \
    's.replace("    branches-ignore: [\x27dependabot/**\x27]\n", "    paths: [\x27sdks/**\x27]\n")'
expect "A: a paths-filtered workflow is refused" 1 "push.paths\` filters" "$d"

d="$(fixture devonly)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("branches-ignore: [\x27dependabot/**\x27]", "branches: [dev]")'
expect "A: a fixed branch list is refused" 1 "does not reach every working branch" "$d"

d="$(fixture tagsonly)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("branches-ignore: [\x27dependabot/**\x27]", "tags-ignore: [\x27v*\x27]")'
expect "A: tags-ignore alone reaches no branch" 1 "does not reach every working branch" "$d"

d="$(fixture nopush)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("  push:\n    branches-ignore: [\x27dependabot/**\x27]\n", "  workflow_dispatch:\n")'
expect "A: no push trigger is refused" 1 "has no \`push:\` trigger" "$d"

d="$(fixture flow)"
edit "$d" .github/workflows/sdk-suites.yml \
    's.replace("  push:\n    branches-ignore: [\x27dependabot/**\x27]\n", "  push: {branches: [\x27**\x27]}\n")'
expect "A: a nested flow collection is FATAL, not skipped" 2 "nested flow collection" "$d"

# B. a twelfth SDK without a row, and a row without a directory.
d="$(fixture newsdk)"; mkdir -p "$d/sdks/official/fraiseql-zig"
expect "B: an SDK directory with no matrix row is refused" 1 "fraiseql-zig has no row" "$d"
d="$(fixture staleRow)"
edit "$d" tools/sdk-suites-matrix.py 's.replace("\"demo\": [\"1\"]", "\"demo\": [\"1\"], \"gone\": [\"1\"]")'
expect "B: a matrix row naming no directory is refused" 1 "row \`gone\` names no" "$d"

# C. no setup step, no suite script.
d="$(fixture nosetup)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("matrix.sdk == \x27demo\x27", "matrix.sdk == \x27other\x27")'
expect "C: an SDK without a setup step is refused" 1 "no setup step for \`demo\`" "$d"
d="$(fixture noscript)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("bash tools/sdk-suite.sh", "echo")'
expect "C: a suite job that never runs the script is refused" 1 "never runs tools/sdk-suite.sh" "$d"

# D. the aggregate.
d="$(fixture noalways)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("    if: ${{ always() }}\n", "")'
expect "D: an aggregate without always() is refused" 1 "must run under \`always()\`" "$d"
d="$(fixture noneeds)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("needs: [changes, suite]", "needs: [changes]")'
expect "D: an aggregate that does not need the suite is refused" 1 "does not need \`suite\`" "$d"
d="$(fixture renamed)"
edit "$d" .github/workflows/sdk-suites.yml 's.replace("    name: SDK suites\n", "    name: sdk\n")'
expect "D: a renamed aggregate is refused" 1 "exactly one job named \`SDK suites\`, found 0" "$d"
d="$(fixture unrequired)"
edit "$d" tools/required-checks.toml 's.replace("  \"SDK suites\",\n", "")'
expect "D: an aggregate missing from required-checks.toml is refused" 1 "gates nothing" "$d"

# E. a filtered per-SDK copy beside the gate.
d="$(fixture copy)"
cat >"$d/.github/workflows/demo-sdk.yml" <<'YML'
on:
  push:
    paths: ['sdks/official/fraiseql-demo/**']
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: bash tools/sdk-suite.sh demo
YML
expect "E: a branch-push copy of the suite is refused" 1 "demo-sdk.yml: job \`test\`" "$d"
d="$(fixture tagpublish)"
cat >"$d/.github/workflows/demo-sdk.yml" <<'YML'
on:
  push:
    tags: ['demo-sdk/v*']
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - run: bash tools/sdk-suite.sh demo
YML
expect "E: a tag-only publish job may run the suite" 0 "all 1 official SDKs" "$d"

d="$(fixture missing)"; rm "$d/.github/workflows/sdk-suites.yml"
expect "the pre-#1467 tree (no sdk-suites.yml) is refused" 1 "sdk-suites.yml is missing" "$d"

expect "the repository's own workflows pass" 0 "official SDKs run under" "$REPO_ROOT"

echo
echo "── the matrix script ──"

repo="$WORK/git"
mkdir -p "$repo" && cd "$repo"
git init -q -b dev . && git config user.email t@t && git config user.name t
mkdir -p sdks/official/fraiseql-go sdks/official/fraiseql-python tools docs
echo a >sdks/official/fraiseql-go/a; echo a >docs/a; echo a >tools/sdk-suite.sh
git add -A && git commit -qm base && base="$(git rev-parse HEAD)"
git update-ref refs/remotes/origin/dev "$base"

sdks_for() { python3 "$MATRIX" "$@" 2>&1 >/dev/null | sed -n 's/.*SDKs: //p'; }
entries_for() {
    python3 "$MATRIX" "$@" 2>/dev/null | python3 -c 'import json,sys;print(len(json.load(sys.stdin)["include"]))'
}

echo b >docs/a && git commit -qam docs
got="$(sdks_for --base "$base" --head HEAD)"
[ "$got" = none ] && pass "a push that touches no SDK runs none" || fail "docs push ran: $got"
[ "$(entries_for --base "$base" --head HEAD)" = 0 ] && pass "…and emits an empty matrix" \
    || fail "docs push emitted entries"

echo b >sdks/official/fraiseql-go/a && echo b >sdks/official/fraiseql-python/b
git add -A && git commit -qm sdks
got="$(sdks_for --base "$base" --head HEAD)"
[ "$got" = "go, python" ] && pass "a push touching two SDKs runs exactly those" || fail "got: $got"
[ "$(entries_for --base "$base" --head HEAD)" = 6 ] && pass "…one entry per toolchain version (2 go + 4 python)" \
    || fail "wrong entry count: $(entries_for --base "$base" --head HEAD)"

got="$(sdks_for --base "0000000000000000000000000000000000000000" --head HEAD)"
[ "$got" = "go, python" ] && pass "a new branch diffs against its merge-base with origin/dev" \
    || fail "got: $got"

prev="$(git rev-parse HEAD)"
echo b >tools/sdk-suite.sh && git commit -qam suite
got="$(sdks_for --base "$prev" --head HEAD)"
[ "$got" = "$(python3 "$MATRIX" --sdks | paste -sd, | sed 's/,/, /g')" ] \
    && pass "a change to the suite definition runs every SDK" || fail "got: $got"

got="$(sdks_for --base deadbeef --head HEAD)"
case "$got" in *typescript*) pass "an unresolvable base runs every SDK (fails toward running)" ;;
    *) fail "unresolvable base ran: $got" ;; esac
cd "$REPO_ROOT"

echo
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "sdk workflow coverage self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "sdk workflow coverage self-test: $TESTS_RUN/$TESTS_RUN passed"
