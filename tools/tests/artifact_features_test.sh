#!/usr/bin/env bash
# Red-capability pin for tools/check-artifact-features.py.
#
# Run directly:  bash tools/tests/artifact_features_test.sh
# Exits non-zero if any assertion fails.
#
# The gate holds six hand-written copies of the platform artifacts' feature list to
# the one adopters read in docs/releases.md (#1621). Each case below drops or adds a
# feature in ONE copy and expects a parity failure (exit 1); each unreadable shape
# expects a fatal (exit 2), because a recipe the gate silently skipped would report
# a parity it never checked. The two passing mutations prove the comparison is on the
# compiled closure, not the literal list.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FILES=(
    tools/check-artifact-features.py
    tools/check-image-parity.py
    tools/check-suite-coverage.py
    crates/fraiseql/Cargo.toml
    crates/fraiseql-server/Cargo.toml
    docs/releases.md
    .dagger/image.go
    .github/workflows/release.yml
    .github/workflows/release-smoke.yml
)

# A fixture repo carrying only what the gate reads.
make_fixture() {
    local dir="$1" f
    for f in "${FILES[@]}"; do
        mkdir -p "$dir/$(dirname "$f")"
        cp "$REPO_ROOT/$f" "$dir/$f"
    done
}

fixture_sum() { ( cd "$1" && cat "${FILES[@]}" 2>/dev/null | cksum ); }

# expect <pass|fail|fatal> <case-id> <description> <must-say> <command...>
# The command mutates the fixture (run inside it) before the gate runs. <must-say> is
# an extended regex the gate's output has to match, so a case cannot go red for a reason
# other than the one it names ("" for the passing cases).
expect() {
    local want="$1" id="$2" desc="$3" says="$4"; shift 4
    local dir="$WORK/$id" code got before
    TESTS_RUN=$((TESTS_RUN + 1))
    make_fixture "$dir"
    before="$(fixture_sum "$dir")"
    ( cd "$dir" && "$@" ) || { printf '  ❌ %-4s mutation itself failed\n' "$id"; TESTS_FAILED=$((TESTS_FAILED+1)); return; }
    if [ "$*" != noop ] && [ "$(fixture_sum "$dir")" = "$before" ]; then
        printf '  ❌ %-4s mutation did not change the fixture\n' "$id"; TESTS_FAILED=$((TESTS_FAILED+1)); return
    fi

    code=0
    ( cd "$dir" && python3 tools/check-artifact-features.py >"$dir/.out" 2>&1 ) || code=$?
    case "$code" in
        0) got=pass ;;
        1) got=fail ;;
        2) got=fatal ;;
        *) got="exit $code" ;;
    esac

    if [ "$got" = "$want" ] && { [ -z "$says" ] || grep -qE -- "$says" "$dir/.out"; }; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s saying %q, got %s\n' "$id" "$desc" "$want" "$says" "$got"
        sed 's/^/        /' "$dir/.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

noop() { :; }

echo "artifact-features gate self-test"
echo

echo "── the unmutated tree passes (or every assertion below is vacuous) ──"
expect pass P0 "repository as it stands" "" noop

echo
echo "── a recipe drops a promised feature ──"
expect fail R1 "umbrella federation stops forwarding to the server (the #1621 shape)" 'crates/fraiseql/Cargo.toml `release-full` does not compile: federation' \
    sed -i 's|^federation = \["fraiseql-core/federation", "fraiseql-server?/federation"\]|federation = ["fraiseql-core/federation"]|' crates/fraiseql/Cargo.toml
expect fail R2 "release-full drops rest" 'crates/fraiseql/Cargo.toml `release-full` does not compile: rest' \
    sed -i '/^release-full = \[/,/^\]/{/^  "rest",$/d}' crates/fraiseql/Cargo.toml
expect fail R3 "one release.yml server build drops cdc-outbound" '\.github/workflows/release\.yml:[0-9]+ does not compile: cdc-outbound' \
    sed -i '0,/--package fraiseql-server --features rest,arrow,cdc-outbound,/s//--package fraiseql-server --features rest,arrow,/' .github/workflows/release.yml
expect fail R4 "the release-smoke boot build drops arrow" '\.github/workflows/release-smoke\.yml:[0-9]+ does not compile: arrow' \
    sed -i 's|-p fraiseql-server --features rest,arrow,|-p fraiseql-server --features rest,|' .github/workflows/release-smoke.yml
expect fail R5 "the multi-line arm64 build drops federation" '\.github/workflows/release-smoke\.yml:[0-9]+ does not compile: federation' \
    sed -i 's|^\( *--features rest,arrow,cdc-outbound,.*\),federation$|\1|' .github/workflows/release-smoke.yml
expect fail R6 "the platform image drops cdc-outbound" 'fraiseql-server-platform` does not compile: cdc-outbound' \
    sed -i '/name: "fraiseql-server-platform"/s|CARGO_FEATURES=rest,arrow,cdc-outbound,|CARGO_FEATURES=rest,arrow,|' .dagger/image.go

echo
echo "── a recipe compiles what the docs do not promise ──"
expect fail R7 "the docs row stops listing rest" 'does not promise: rest' \
    sed -i '/^| `fraiseql-full-<target>`/s|the `rest` and `arrow`|the `arrow`|' docs/releases.md
expect fail R8 "a release.yml server build adds grpc" '\.github/workflows/release\.yml:[0-9]+ compiles what docs/releases.md does not promise: grpc' \
    sed -i '0,/--package fraiseql-server --features rest,/s//--package fraiseql-server --features grpc,rest,/' .github/workflows/release.yml

echo
echo "── the comparison is on the compiled closure, not the literal list ──"
expect pass P1 "the image omits inbound, which inbound-email implies" "" \
    sed -i '/name: "fraiseql-server-platform"/s|,inbound,inbound-email,|,inbound-email,|' .dagger/image.go
expect pass P2 "the docs row also names functions-runtime, which functions-runtime-deno implies" "" \
    sed -i '/^| `fraiseql-full-<target>`/s|(`functions-runtime-deno`)|(`functions-runtime-deno`, `functions-runtime`)|' docs/releases.md

echo
echo "── a shape the gate cannot read is fatal, not skipped ──"
expect fatal F1 "the docs row is gone" 'expected exactly one table row' \
    sed -i '/^| `fraiseql-full-<target>`/d' docs/releases.md
expect fatal F2 "the docs row names a feature the umbrella does not have" '`metricz` is not a feature of the fraiseql crate' \
    sed -i '/^| `fraiseql-full-<target>`/s|`metrics`|`metricz`|' docs/releases.md
expect fatal F3 "a workflow loses every featured server build" 'release.yml: no `cargo build -p fraiseql-server' \
    sed -i '/fraiseql-server --features/d' .github/workflows/release.yml
expect fatal F4 "a server build takes its features from an expression" 'release\.yml:[0-9]+: the build takes its features from an expression' \
    sed -i '0,/--package fraiseql-server --features rest,[^ ]*/s//--package fraiseql-server --features ${{ env.F }}/' .github/workflows/release.yml
expect fatal F5 "the platform image variant is gone" 'has no '"'"'fraiseql-server-platform'"'"' variant' \
    sed -i '/name: "fraiseql-server-platform"/d' .dagger/image.go
expect fatal F6 "a server recipe names a feature fraiseql-server does not have" '`restt` is not a fraiseql-server feature' \
    sed -i '/name: "fraiseql-server-platform"/s|CARGO_FEATURES=rest,|CARGO_FEATURES=restt,|' .dagger/image.go
expect fatal F7 "the docs row appears twice, so which one is the promise is ambiguous" 'expected exactly one table row' \
    sed -i '/^| `fraiseql-full-<target>`/p' docs/releases.md

expect fatal F8 "a server feature names an optional dependency without dep:" 'names the optional dependency `rmcp` without `dep:`' \
    sed -i 's|"dep:rmcp"|"rmcp"|' crates/fraiseql-server/Cargo.toml
expect fatal F9 "an optional dependency no feature names as dep:" 'the optional dependency `zz-unwired` is named by no feature' \
    sed -i '0,/^\[dependencies\]$/s//[dependencies]\nzz-unwired = { version = "1", optional = true }/' crates/fraiseql-server/Cargo.toml

expect fatal F10 "a feature only re-enables the optional dependency of its own name" 'the feature `rmcp` only re-enables the optional dependency `rmcp`' \
    sed -i 's|^webhooks = \["dep:fraiseql-webhooks"\]$|webhooks = ["dep:fraiseql-webhooks"]\nrmcp = ["dep:rmcp"]|' crates/fraiseql-server/Cargo.toml
echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ all $TESTS_RUN assertions passed"
