#!/usr/bin/env bash
# Unit tests for tools/check-dockerfile-msrv.sh.
#
# Run directly:  bash tools/tests/dockerfile_msrv_test.sh
# Exits non-zero if any assertion fails.
#
# The gate couples every Dockerfile's Rust base image to [workspace.package]
# rust-version (#1107). Its boundary behaviour is what needs pinning, because both
# directions of error are silent:
#
#   – too strict, and it flags the floating tags that most of this tree uses
#     (rust:latest, rust:1-slim-bookworm) — a gate that is wrong about the majority
#     of its inputs gets disabled;
#   – too lax, and a stale pin ships. docker-build.yml is tag-only, so the first
#     witness to a bad pin is the release itself.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-dockerfile-msrv.sh"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# assert_base <name> <expected-exit> <from-line> [msrv]
assert_base() {
    local name="$1" want_exit="$2" from_line="$3" msrv="${4:-1.94.1}"
    TESTS_RUN=$((TESTS_RUN + 1))
    local dir="$WORK/$name"
    mkdir -p "$dir"
    printf '[workspace.package]\nrust-version = "%s"  # MSRV — trailing prose\n' "$msrv" >"$dir/Cargo.toml"
    printf '%s\nRUN cargo build\n' "$from_line" >"$dir/Dockerfile"

    local out rc
    set +e
    out="$(DOCKERFILE_MSRV_ROOT="$dir" bash "$GATE" 2>&1)"
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

echo "=== check-dockerfile-msrv.sh ==="

# ── The defect itself: the literal #1107 state ───────────────────────────────────
assert_base "stale-pin-1.92-is-rejected"        1 'FROM rust:1.92-slim AS builder'
assert_base "exact-msrv-passes"                 0 'FROM rust:1.94.1-slim AS builder'
assert_base "newer-pin-passes"                  0 'FROM rust:1.95.0-slim AS builder'
assert_base "platform-flag-is-parsed"           1 'FROM --platform=$BUILDPLATFORM rust:1.92-slim AS builder'

# ── Floating tags must stay allowed, or the gate gets disabled ───────────────────
assert_base "latest-is-allowed"                 0 'FROM rust:latest as builder'
assert_base "floating-major-is-allowed"         0 'FROM rust:1-slim-bookworm AS builder'
assert_base "nightly-is-allowed"                0 'FROM rust:nightly AS builder'

# ── The two-component boundary Cargo.toml calls out by name ─────────────────────
# `rust:1.94` floats the PATCH — Docker resolves it to the newest 1.94.x, so it
# satisfies a 1.94.1 MSRV even though the string "1.94" sorts below "1.94.1".
assert_base "floating-patch-same-minor-passes"  0 'FROM rust:1.94-slim AS builder'
assert_base "floating-patch-older-minor-fails"  1 'FROM rust:1.93-slim AS builder'
# But a fully-pinned older patch is a real floor violation: "a 1.94.0 floor was a
# claim no leg ever tested".
assert_base "pinned-older-patch-fails"          1 'FROM rust:1.94.0-slim AS builder'

# ── Versions compare as versions, not as strings ────────────────────────────────
# Lexicographically "1.100.0" < "1.94.1". This is the classic way such a check
# starts rejecting the future.
assert_base "double-digit-minor-compares-numerically" 0 'FROM rust:1.100.0-slim AS builder'

# ── Fail closed on what cannot be resolved ──────────────────────────────────────
assert_base "arg-indirected-tag-is-rejected"    1 'FROM rust:${RUST_VERSION}-slim AS builder'

# ── Scope ───────────────────────────────────────────────────────────────────────
# A non-Rust base is not this gate's business, but the tree still has a Dockerfile,
# so the vacuous-scan guard must not fire.
assert_base "non-rust-base-is-ignored"          0 'FROM debian:bookworm-slim'

# ── The vacuous-scan guard ──────────────────────────────────────────────────────
TESTS_RUN=$((TESTS_RUN + 1))
empty="$WORK/no-dockerfiles"
mkdir -p "$empty"
printf '[workspace.package]\nrust-version = "1.94.1"\n' >"$empty/Cargo.toml"
set +e
DOCKERFILE_MSRV_ROOT="$empty" bash "$GATE" >/dev/null 2>&1
rc=$?
set -e
if [ "$rc" -ne 1 ]; then
    echo "  FAIL: no-dockerfile-found-is-a-failure — expected exit 1, got $rc"
    TESTS_FAILED=$((TESTS_FAILED + 1))
else
    echo "  ok: no-dockerfile-found-is-a-failure"
fi

# ── An unreadable MSRV must fail, not default ───────────────────────────────────
TESTS_RUN=$((TESTS_RUN + 1))
nomsrv="$WORK/no-msrv"
mkdir -p "$nomsrv"
printf '[workspace.package]\nversion = "2.15.0"\n' >"$nomsrv/Cargo.toml"
printf 'FROM rust:1.92-slim\n' >"$nomsrv/Dockerfile"
set +e
DOCKERFILE_MSRV_ROOT="$nomsrv" bash "$GATE" >/dev/null 2>&1
rc=$?
set -e
if [ "$rc" -ne 1 ]; then
    echo "  FAIL: missing-rust-version-is-a-failure — expected exit 1, got $rc"
    TESTS_FAILED=$((TESTS_FAILED + 1))
else
    echo "  ok: missing-rust-version-is-a-failure"
fi

echo ""
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "FAILED: $TESTS_FAILED of $TESTS_RUN assertions"
    exit 1
fi
echo "OK: $TESTS_RUN/$TESTS_RUN assertions passed"
