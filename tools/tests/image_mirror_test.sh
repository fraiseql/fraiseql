#!/usr/bin/env bash
# Unit tests for tools/check-image-mirror.py (#1453).
#
# Run directly:  bash tools/tests/image_mirror_test.sh
# Exits non-zero if any assertion fails.
#
# One fixture per refusal, each rejected by that rule alone, plus the clean shapes
# (a version tag, a digest), so a gate that refuses everything or nothing fails here.
#
# Exit codes of the gate: 0 = clean, 1 = findings, 2 = FATAL.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-image-mirror.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

DIGEST="sha256:$(printf 'a%.0s' $(seq 64))"

# fixture <name> <go-image-literal> <mirror-rows…> → prints the root dir
fixture() {
    local dir="$WORK/$1" image="$2"
    shift 2
    mkdir -p "$dir/.github/workflows" "$dir/.dagger"
    printf 'package main\n\nconst (\n\t// "ghcr.io/fraiseql/commented:latest" is prose\n\timg = "%s"\n)\n' \
        "$image" >"$dir/.dagger/main.go"
    {
        printf 'jobs:\n  mirror:\n    steps:\n      - run: |\n          IMAGES="\n'
        printf '          %s\n' "$@"
        printf '          "\n'
    } >"$dir/.github/workflows/mirror-base-images.yml"
    echo "$dir"
}

# expect <label> <want-exit> <want-substring> <root>
expect() {
    TESTS_RUN=$((TESTS_RUN + 1))
    local out code=0
    out="$(IMAGE_MIRROR_ROOT="$4" python3 "$GATE" 2>&1)" || code=$?
    if [ "$code" != "$2" ] || ! grep -qF -- "$3" <<<"$out"; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        echo "FAIL  $1 (exit $code, wanted $2 and \"$3\")"
        printf '%s\n' "$out" | sed 's/^/        /'
    else
        echo "PASS  $1"
    fi
}

expect "a version-tagged mirror row is clean" 0 "image mirror: OK" \
    "$(fixture tag ghcr.io/fraiseql/redis:7-alpine 'docker.io/library/redis:7-alpine|ghcr.io/fraiseql/redis:7-alpine')"
expect "a digest-pinned source is clean" 0 "image mirror: OK" \
    "$(fixture digest ghcr.io/fraiseql/minio:R1 "cgr.dev/chainguard/minio@$DIGEST|ghcr.io/fraiseql/minio:R1")"

# A: the Dagger module pulls a tag nothing mirrors.
expect "an unmirrored Dagger image is refused" 1 "is not a destination" \
    "$(fixture unmirrored ghcr.io/fraiseql/redis:7-alpine 'docker.io/library/redis:7|ghcr.io/fraiseql/redis:7')"

# B: the #1453 shape — a floating source and destination.
expect "a :latest source is refused" 1 "mirror source docker.io/minio/minio:latest rides" \
    "$(fixture latest-src ghcr.io/fraiseql/minio:R1 'docker.io/minio/minio:latest|ghcr.io/fraiseql/minio:R1')"
expect "a :latest destination is refused" 1 "mirror destination ghcr.io/fraiseql/minio:latest rides" \
    "$(fixture latest-dst ghcr.io/fraiseql/minio:latest "cgr.dev/chainguard/minio@$DIGEST|ghcr.io/fraiseql/minio:latest")"
expect "an untagged source is refused" 1 "has no tag" \
    "$(fixture untagged ghcr.io/fraiseql/minio:R1 'docker.io/minio/minio|ghcr.io/fraiseql/minio:R1')"
expect "a registry port is not a tag" 1 "has no tag" \
    "$(fixture port ghcr.io/fraiseql/minio:R1 'localhost:5000/minio|ghcr.io/fraiseql/minio:R1')"

# Inputs the gate cannot read are FATAL, never clean.
expect "a malformed mirror row is FATAL" 2 "unreadable mirror row" \
    "$(fixture malformed ghcr.io/fraiseql/redis:7 'docker.io/library/redis:7')"
dir="$(fixture nodagger ghcr.io/fraiseql/redis:7 'docker.io/library/redis:7|ghcr.io/fraiseql/redis:7')"
rm "$dir/.dagger/main.go"
expect "a scan with no Dagger image is FATAL" 2 "found no ghcr.io/fraiseql image" "$dir"

# The live tree.
expect "the repository's own mirror is clean" 0 "image mirror: OK" "$REPO_ROOT"

echo
echo "$TESTS_RUN tests, $TESTS_FAILED failed"
[ "$TESTS_FAILED" -eq 0 ]
