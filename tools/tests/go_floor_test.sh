#!/usr/bin/env bash
# Unit tests for tools/check-go-floor.py (#2278).
#
# Run directly:  bash tools/tests/go_floor_test.sh
# Exits non-zero if any assertion fails.
#
# One fixture tree the gate accepts, then one mutation per rule, each refused naming the
# file it broke, so a gate that refuses everything or nothing fails here.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$(cd "$SCRIPT_DIR/.." && pwd)/check-go-floor.py"

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

# fixture <name> → a tree on Go 1.26 everywhere.
fixture() {
    local dir="$WORK/$1"
    mkdir -p "$dir/tools" "$dir/sdks/official/fraiseql-go" "$dir/sdks/official/conformance" \
             "$dir/.github/workflows"
    printf 'module example.com/demo\n\ngo 1.26\n' >"$dir/sdks/official/fraiseql-go/go.mod"
    printf 'VERSIONS = {\n    "go": ["1.26", "1.27"],\n}\n' >"$dir/tools/sdk-suites-matrix.py"
    printf "jobs:\n  a:\n    steps:\n      - uses: actions/setup-go@v5\n        with:\n          go-version: '1.26'\n      - uses: actions/setup-go@v5\n        with:\n          go-version: \${{ matrix.version }}\n" \
        >"$dir/.github/workflows/demo.yml"
    printf '{"sdks": {"go": {"container": "golang:1.26"}}}\n' \
        >"$dir/sdks/official/conformance/manifest.json"
    printf '**Requirements**: Go 1.26+\n' >"$dir/sdks/official/fraiseql-go/README.md"
    printf -- '- Go 1.26 or later\n' >"$dir/sdks/official/fraiseql-go/CONTRIBUTING.md"
    printf 'Go Version: 1.26+\n' >"$dir/sdks/official/fraiseql-go/implementation-summary.md"
    printf '| fraiseql-go/ | Go 1.26+ |\n' >"$dir/sdks/official/README.md"
    echo "$dir"
}

# expect <label> <exit> <needle> <dir>
expect() {
    local label=$1 want=$2 needle=$3 dir=$4 out code=0
    out=$(GO_FLOOR_ROOT="$dir" python3 "$GATE" 2>&1) || code=$?
    if [ "$code" -ne "$want" ]; then
        fail "$label: exit $code, wanted $want" "$out"
    elif [ -n "$needle" ] && ! grep -qF -- "$needle" <<<"$out"; then
        fail "$label: output does not name $needle" "$out"
    else
        pass "$label"
    fi
}

dir=$(fixture clean)
expect "a tree on one Go is accepted" 0 "Go 1.26 everywhere" "$dir"

dir=$(fixture go-mod)
sed -i 's/^go 1.26$/go 1.25/' "$dir/sdks/official/fraiseql-go/go.mod"
expect "go.mod alone moved" 1 "tools/sdk-suites-matrix.py" "$dir"

dir=$(fixture matrix)
sed -i 's/"1.26", "1.27"/"1.27"/' "$dir/tools/sdk-suites-matrix.py"
expect "A: the matrix does not test the floor" 1 "the oldest Go tested is 1.27" "$dir"

dir=$(fixture matrix-order)
sed -i 's/"1.26", "1.27"/"1.27", "1.26"/' "$dir/tools/sdk-suites-matrix.py"
expect "A: the matrix's order is not its oldest" 0 "" "$dir"

dir=$(fixture workflow)
sed -i "s/go-version: '1.26'/go-version: '1.24'/" "$dir/.github/workflows/demo.yml"
expect "B: a workflow pins another Go" 1 ".github/workflows/demo.yml:6: go-version 1.24" "$dir"

dir=$(fixture container)
sed -i 's/golang:1.26/golang:1.24-bookworm/' "$dir/sdks/official/conformance/manifest.json"
expect "C: the conformance container runs another Go" 1 "golang:1.24" "$dir"

for doc in fraiseql-go/README.md fraiseql-go/CONTRIBUTING.md \
           fraiseql-go/implementation-summary.md README.md; do
    dir=$(fixture "doc-${doc//\//-}")
    sed -i 's/1\.26/1.22/' "$dir/sdks/official/$doc"
    expect "D: sdks/official/$doc states another floor" 1 "sdks/official/$doc:1" "$dir"
done

dir=$(fixture no-go-line)
printf 'module example.com/demo\n' >"$dir/sdks/official/fraiseql-go/go.mod"
expect "a go.mod with no go line is FATAL" 2 "declares no \`go\` line" "$dir"

dir=$(fixture no-row)
printf 'VERSIONS = {}\n' >"$dir/tools/sdk-suites-matrix.py"
expect "a matrix with no go row is FATAL" 2 "has no \`go\` row" "$dir"

echo "$TESTS_RUN run, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
