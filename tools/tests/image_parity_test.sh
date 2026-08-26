#!/usr/bin/env bash
# Red-capability pin for tools/check-image-parity.py.
#
# Run directly:  bash tools/tests/image_parity_test.sh
# Exits non-zero if any assertion fails.
#
# The gate holds three copies of one list to each other: `docker-build.yml`'s two
# matrices and `.dagger/image.go`'s table. Every drift below is a way an image
# reaches a release having never been built by a pre-tag gate, or a way the leg
# builds something that is not what ships — which is #1205 either way.
#
# Both directions matter and they are not symmetric in consequence, so both are
# asserted: a variant published but not built by the leg is the dangerous one; a
# variant built but published nowhere means the table has stopped describing what
# we ship. And a row the gate cannot parse must be FATAL — the gate reads Go as
# source text, so a reformat that silently dropped a row would report a parity it
# never checked.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A fixture repo carrying only what the gate reads.
make_fixture() {
    local dir="$1"
    mkdir -p "$dir/tools" "$dir/.dagger" "$dir/.github/workflows"
    cp "$REPO_ROOT/tools/check-image-parity.py" "$dir/tools/"
    cp "$REPO_ROOT/tools/check-suite-coverage.py" "$dir/tools/"
    cp "$REPO_ROOT/.dagger/image.go" "$dir/.dagger/"
    cp "$REPO_ROOT/.github/workflows/docker-build.yml" "$dir/.github/workflows/"
}

# expect <pass|fail> <case-id> <description> [sed-style mutation...]
# The mutation is applied to the fixture before running the gate.
expect() {
    local want="$1" id="$2" desc="$3"; shift 3
    local dir="$WORK/$id" got
    TESTS_RUN=$((TESTS_RUN + 1))
    make_fixture "$dir"
    ( cd "$dir" && "$@" ) || { printf '  ❌ %-4s mutation itself failed\n' "$id"; TESTS_FAILED=$((TESTS_FAILED+1)); return; }

    if ( cd "$dir" && python3 tools/check-image-parity.py >"$dir/.out" 2>&1 ); then
        got=pass
    else
        got=fail
    fi

    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$desc" "$want" "$got"
        sed 's/^/        /' "$dir/.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

noop() { :; }

echo "image-parity gate self-test"
echo

echo "── the unmutated tree passes (or every assertion below is vacuous) ──"
expect pass P0 "repository as it stands" noop

echo
echo "── a variant published but not built before the tag ──"
expect fail R1 "variant dropped from the Dagger table" \
    sed -i '/name: "tutorial", dockerfile: "tutorial\/Dockerfile"/d' .dagger/image.go

echo
echo "── a variant built by the leg that nothing publishes ──"
expect fail R2 "variant added to the Dagger table only" \
    sed -i 's|{name: "tutorial", dockerfile: "tutorial/Dockerfile", buildContext: ".", buildArgs: "", optional: true},|&\n\t{name: "ghost", dockerfile: "Dockerfile", buildContext: ".", buildArgs: "", optional: true},|' .dagger/image.go

echo
echo "── the leg builds the right name with the wrong inputs ──"
expect fail R3 "build-args dropped from the Dagger table" \
    sed -i 's|buildArgs: "CARGO_FEATURES=rest,arrow"|buildArgs: ""|' .dagger/image.go
expect fail R4 "a different Dockerfile in the Dagger table" \
    sed -i 's|{name: "tutorial", dockerfile: "tutorial/Dockerfile"|{name: "tutorial", dockerfile: "Dockerfile"|' .dagger/image.go
expect fail R5 "optional flag disagrees with the matrix" \
    sed -i 's|{name: "fraiseql-server", dockerfile: "Dockerfile", buildContext: ".", buildArgs: "", optional: false}|{name: "fraiseql-server", dockerfile: "Dockerfile", buildContext: ".", buildArgs: "", optional: true}|' .dagger/image.go

echo
echo "── the two registries disagree with each other ──"
expect fail R6 "build-args present for ghcr, absent for Docker Hub" \
    sed -i '0,/            build-args: "CARGO_FEATURES=rest,arrow"/s///' .github/workflows/docker-build.yml

echo
echo "── a shape the gate cannot read is fatal, not skipped ──"
expect fail R7 "an imageVariants row the parser cannot match" \
    sed -i 's|{name: "tutorial", dockerfile: "tutorial/Dockerfile", buildContext: ".", buildArgs: "", optional: true},|{Name: "tutorial", DockerFile: "tutorial/Dockerfile"},|' .dagger/image.go
expect fail R8 "the table is gone entirely" \
    sed -i '/^var imageVariants = \[\]imageVariant{$/,/^}$/d' .dagger/image.go
expect fail R9 "the workflow the gate compares against is gone" \
    rm .github/workflows/docker-build.yml

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ all $TESTS_RUN assertions passed"
