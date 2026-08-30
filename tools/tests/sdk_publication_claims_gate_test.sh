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

# fixture <id> [bumped] [readme-registry-for-alpha] [publisher] [gated] [trigger] [jobif]
#
# Two SDKs, `alpha` and `beta`. By default alpha is published consistently across all
# three artifacts and beta is source-only — the shape the repository is now in.
#
# `trigger` selects the publisher workflow's `on:` block, which is what decides whether
# the release tag reaches it:
#   release   push: tags: ['v*']          — the tag release.sh creates
#   sdktag    push: tags: ['alpha-sdk/v*'] — a tag nobody pushes (the Rust SDK's shape)
#   branches  push: branches + paths      — a tag matches no ref pattern (#1119)
#   paths     push: paths only            — path filters are not evaluated for tag pushes
fixture() {
    local id="$1"
    local bumped="${2:-yes}" registry="${3:-PyPI}" publisher="${4:-yes}" gated="${5:-yes}"
    local trigger="${6:-release}" jobif="${7:-}"
    local dir="$WORK/$id"
    mkdir -p "$dir/sdks/official/fraiseql-alpha" "$dir/sdks/official/fraiseql-beta" \
             "$dir/tools" "$dir/.github/workflows"

    # The new check reads the tag a release creates rather than assuming one.
    {
        echo 'RELEASE_FILES=('
        echo '    Cargo.toml'
        [ "$bumped" = yes ] && echo '    sdks/official/fraiseql-alpha/pyproject.toml'
        echo ')'
        echo 'git tag -a "v${VERSION}" -m "$TAG_MSG"'
    } > "$dir/tools/release.sh"

    # Reachability is decided from parsed YAML, and the parser lives in a sibling gate.
    cp "$REPO_ROOT/tools/check-suite-coverage.py" "$dir/tools/"

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
            echo 'on:'
            echo '  push:'
            case "$trigger" in
                release)  echo "    tags: ['v*']" ;;
                sdktag)   echo "    tags: ['alpha-sdk/v*']" ;;
                branches) echo "    branches: ['**']"; echo "    paths: ['sdks/official/fraiseql-alpha/**']" ;;
                paths)    echo "    paths: ['sdks/official/fraiseql-alpha/**']" ;;
                *)        echo "unknown trigger $trigger" >&2; exit 1 ;;
            esac
            echo 'jobs:'
            echo '  publish:'
            [ -n "$jobif" ] && echo "    if: $jobif"
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
echo "── a published SDK must have a publisher the release tag reaches ──"
# The defect this section pins: every check above passes while the SDK stays at 404,
# because agreement says an SDK is MEANT to be published, not that a release does it.
expect fail E0 "publisher fires only on an alpha-sdk/v* tag release.sh never creates" \
    "$(fixture e0 yes PyPI yes yes sdktag)"
# #1119: with `branches` present and `tags` absent a tag push matches no ref pattern,
# so the workflow does not start at all — the shape csharp-sdk.yml was in.
expect fail E1 "publisher workflow has branches+paths and no tags filter" \
    "$(fixture e1 yes PyPI yes yes branches)"
# The converse, and the reason the deleted publishers were dangerous: GitHub does not
# evaluate path filters for tag pushes, so a paths-only workflow runs on every tag.
expect pass E2 "publisher workflow has a paths filter only — tags still reach it" \
    "$(fixture e2 yes PyPI yes yes paths)"
expect pass E3 "job if names the release tag prefix" \
    "$(fixture e3 yes PyPI yes yes release "startsWith(github.ref, 'refs/tags/v')")"
expect fail E4 "job if names a different tag prefix" \
    "$(fixture e4 yes PyPI yes yes release "startsWith(github.ref, 'refs/tags/alpha-sdk/v')")"
expect fail E5 "job if names a non-push event" \
    "$(fixture e5 yes PyPI yes yes release "github.event_name == 'release'")"
# Positive proof: an expression the gate cannot evaluate is not a reachable publisher.
expect fail E6 "job if uses a shape the gate cannot evaluate" \
    "$(fixture e6 yes PyPI yes yes release "contains(github.ref, 'v') || always()")"
d="$(fixture e7)"; sed -i '/git tag -a/d' "$d/tools/release.sh"
expect fail E7 "release.sh creates no tag the gate can read" "$d"
d="$(fixture e8)"; rm -f "$d/tools/check-suite-coverage.py"
expect fail E8 "the YAML parser is missing — no silent skip" "$d"

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
