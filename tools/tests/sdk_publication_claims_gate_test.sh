#!/usr/bin/env bash
# Self-test for tools/check-sdk-publication-claims.py — the red-capability pin for the
# #1130 gate.
#
# The defect that gate exists to prevent is a *claim* drifting from *machinery*, so the
# suite has to vary each of the three artifacts independently and check that moving any
# one of them alone turns the gate red. A fixture that changes two at once would pass
# under a gate that only reads one.
#
# Fixtures are whole throwaway trees in a temp dir. The gate is never pointed at this
# repository with a mutation applied, so an interrupted run cannot leave the tree edited.
#
# SDK_CLAIMS_GATE points the suite at another copy of the gate, which is how a case is
# shown RED against a revision that could not see it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="${SDK_CLAIMS_GATE:-$REPO_ROOT/tools/check-sdk-publication-claims.py}"

[ -f "$GATE" ] || { echo "❌ missing $GATE"; exit 1; }

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# fixture <id> [bumped] [readme-registry-for-alpha] [publisher] [gated]
#
# Two SDKs, `alpha` and `beta`. By default alpha is published consistently across all
# three artifacts and beta is source-only — the shape the repository is now in.
fixture() {
    local id="$1"
    local bumped="${2:-yes}" registry="${3:-PyPI}" publisher="${4:-yes}" gated="${5:-yes}"
    local dir="$WORK/$id"
    mkdir -p "$dir/sdks/official/fraiseql-alpha" "$dir/sdks/official/fraiseql-beta" \
             "$dir/tools" "$dir/.github/workflows"

    {
        echo 'RELEASE_FILES=('
        echo '    Cargo.toml'
        [ "$bumped" = yes ] && echo '    sdks/official/fraiseql-alpha/pyproject.toml'
        echo ')'
    } > "$dir/tools/release.sh"

    {
        echo '<!-- sdk-table:start -->'
        echo '| SDK | Language | Registry | Install |'
        echo '|-----|----------|----------|---------|'
        echo "| \`fraiseql-alpha\` | Alpha | ${registry} | x |"
        echo '| `fraiseql-beta` | Beta | — | vendor |'
        echo '<!-- sdk-table:end -->'
    } > "$dir/README.md"

    if [ "$publisher" = yes ]; then
        {
            echo 'jobs:'
            echo '  publish:'
            echo '    steps:'
            [ "$gated" = yes ] && echo '      - run: assert_sdk_version_matches "$M" "$T" "Alpha"'
            echo '      - run: uv publish'
            echo '        working-directory: sdks/official/fraiseql-alpha'
        } > "$dir/.github/workflows/alpha-sdk.yml"
    fi
    echo "$dir"
}

# expect <pass|fail> <case-id> <description> <dir>
expect() {
    local want="$1" id="$2" desc="$3" dir="$4" got
    TESTS_RUN=$((TESTS_RUN + 1))
    if SDK_CLAIMS_ROOT="$dir" python3 "$GATE" >/dev/null 2>&1; then got=pass; else got=fail; fi
    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$desc" "$want" "$got"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "SDK publication-claims gate self-test"
echo
echo "── the three artifacts must agree ──"
expect pass A0 "published in all three, source-only in none" \
    "$(fixture a0)"
# Each of the next three moves exactly ONE artifact. A gate reading only two of them
# would stay green on the one it does not read.
expect fail A1 "README claims a registry the bump set does not cover" \
    "$(fixture a1 no PyPI no)"
expect fail A2 "a publish job for an SDK no release bumps — the #1130 hazard" \
    "$(fixture a2 no PyPI yes)"
expect fail A3 "release.sh bumps an SDK the README calls source-only" \
    "$(fixture a3 yes "—" no)"

echo
echo "── a publisher must assert its manifest matches the tag ──"
# rust-sdk.yml and python-sdk.yml both shipped without this, so a direct SDK tag
# bypassed the H30 gate release.yml applies (#1130).
expect fail B0 "a publish job with no assert_sdk_version_matches" \
    "$(fixture b0 yes PyPI yes no)"

echo
echo "── the gate refuses to pass vacuously ──"
# Each of these removes the gate's ability to see one artifact. Reporting OK over an
# input it could not read is the fabricated-success shape (#1206).
d="$(fixture c0)"; rm -rf "$d/sdks/official"
expect fail C0 "no sdks/official/ directory at all" "$d"
d="$(fixture c1)"; rm -rf "$d/sdks/official/fraiseql-alpha" "$d/sdks/official/fraiseql-beta"
expect fail C1 "sdks/official/ exists but is empty" "$d"
d="$(fixture c2)"; sed -i 's/RELEASE_FILES=(/RENAMED_FILES=(/' "$d/tools/release.sh"
expect fail C2 "release.sh has no RELEASE_FILES array" "$d"
d="$(fixture c3)"; sed -i '/sdk-table:start/d' "$d/README.md"
expect fail C3 "README has no sdk-table marker" "$d"

echo
echo "── a name that is not an SDK ──"
d="$(fixture d0)"
sed -i 's|fraiseql-alpha/pyproject.toml|fraiseql-ghost/pyproject.toml|' "$d/tools/release.sh"
expect fail D0 "release.sh bumps an SDK directory that does not exist" "$d"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ all $TESTS_RUN assertions passed"
