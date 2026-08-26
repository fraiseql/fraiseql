#!/usr/bin/env bash
# Unit tests for tools/check-dockerfile-workspace-members.sh.
#
# Run directly:  bash tools/tests/dockerfile_workspace_members_test.sh
# Exits non-zero if any assertion fails.
#
# The gate asserts every `[workspace] members` entry reaches the release Dockerfile's
# builder stage. cargo loads the whole workspace manifest before building anything, so a
# member that was never COPYed is not a partial build — the image cannot be built at all,
# and nothing in CI builds this file before the tag.
#
# The assertions that matter are the ones where a wrong verdict is silent: a `COPY --from=`
# (which copies between stages, not from the build context) must not count as coverage, a
# COPY in a LATER stage must not count for the builder, and a member excluded by
# .dockerignore is copied as nothing while the COPY line still reads as present.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-dockerfile-workspace-members.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# assert_tree <name> <expected-exit> <members-toml-body> <dockerfile-body> [dockerignore]
assert_tree() {
    local name="$1" want_exit="$2" members="$3" dockerfile="$4" dockerignore="${5:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    mkdir -p "$dir"
    printf '[workspace]\nmembers = [\n%s\n]\n' "$members" >"$dir/Cargo.toml"
    printf '%s\n' "$dockerfile" >"$dir/Dockerfile"
    [ -n "$dockerignore" ] && printf '%s\n' "$dockerignore" >"$dir/.dockerignore"

    local out rc
    set +e
    out="$(DOCKERFILE_MEMBERS_ROOT="$dir" bash "$GATE" 2>&1)"
    rc=$?
    set -e

    if [ "$rc" -ne "$want_exit" ]; then
        echo "  FAIL: $name — expected exit $want_exit, got $rc"
        echo "$out" | sed 's/^/        /'
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "  ok: $name"
    fi
}

echo "=== check-dockerfile-workspace-members.sh ==="

BUILDER='FROM rust:1.94.1-slim AS builder
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
FROM debian:bookworm-slim AS runtime
COPY --from=builder /build/target/release/fraiseql-server .'

# ── The defect itself ────────────────────────────────────────────────────────────
assert_tree "uncopied-member-is-rejected" 1 \
'  "crates/fraiseql-server",
  "examples/basic-query"' "$BUILDER"

assert_tree "all-members-covered-passes" 0 \
'  "crates/fraiseql-server",
  "examples/basic-query"' \
'FROM rust:1.94.1-slim AS builder
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY examples ./examples'

# A parent directory covers its members; that is how crates/* is covered today.
assert_tree "parent-directory-covers-members" 0 \
'  "crates/a",
  "crates/b"' \
'FROM rust:1.94.1-slim AS builder
COPY crates ./crates'

assert_tree "copy-dot-covers-everything" 0 \
'  "crates/a",
  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY . .'

# ── Comments inside the members array must not be read as paths ─────────────────
assert_tree "comments-in-members-array-are-not-members" 0 \
'  "crates/a",  # Root umbrella crate
  # Runnable examples, members so clippy covers them
  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY crates ./crates
COPY examples ./examples'

# ── COPY --from= is a stage copy, not build context ─────────────────────────────
assert_tree "copy-from-stage-does-not-count-as-coverage" 1 \
'  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY Cargo.toml ./
COPY --from=other examples ./examples'

# ── A COPY in a later stage does not feed the builder ───────────────────────────
assert_tree "copy-in-a-later-stage-does-not-count" 1 \
'  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY crates ./crates
FROM debian:bookworm-slim AS runtime
COPY examples ./examples'

# ── .dockerignore silently empties a COPY ───────────────────────────────────────
assert_tree "member-excluded-by-dockerignore-is-rejected" 1 \
'  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY examples ./examples' \
'examples/'

assert_tree "unrelated-dockerignore-entry-is-fine" 0 \
'  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY examples ./examples' \
'docs/
*.md
target/'

# ── COPY flags must not be mistaken for sources ─────────────────────────────────
assert_tree "chown-flag-is-not-a-source" 0 \
'  "examples/b"' \
'FROM rust:1.94.1-slim AS builder
COPY --chown=1000:1000 examples ./examples'

# ── Vacuous-scan guards ─────────────────────────────────────────────────────────
assert_tree "no-members-parsed-is-a-failure" 1 '' \
'FROM rust:1.94.1-slim AS builder
COPY . .'

assert_tree "no-builder-copy-is-a-failure" 1 \
'  "crates/a"' \
'FROM rust:1.94.1-slim AS builder
RUN echo no copies here'

echo ""
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "OK: $TESTS_RUN/$TESTS_RUN assertions passed"
