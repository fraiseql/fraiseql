#!/usr/bin/env bash
# check-example-crates-compile.sh — every standalone Cargo project under examples/
# compiles, including its tests.
#
# Background (#1200): `examples/rust/flight_client` called `do_get` with no
# `authorization` metadata and no handshake, so every call it could make was
# refused — and nothing in the repository would have noticed if it had stopped
# compiling either. Each of these projects declares its own `[workspace]`, which
# puts it outside the main one:
#
#   * `cargo check --workspace`, `cargo clippy --all-targets` and every test leg
#     skip them by construction — the same blindness `crates/*/fuzz` had (#1254);
#   * `check-examples-compile.sh` compiles example *authoring artifacts*
#     (schema.json / fraiseql.toml), not Rust crates;
#   * `check-examples-integrity.sh` is a static gate with no toolchain.
#
# So an example a reader is invited to `cargo run` could rot at `error[E0308]`
# indefinitely. Whether it compiles is deterministic, so it is a merge gate.
#
# Discovery is at runtime, never a hard-coded list: a new example crate is covered
# the day it lands, and an empty discovery is a failure rather than a silent pass.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

SCAN_ROOT="${EXAMPLE_CRATES_SCAN_ROOT:-examples}"

if ! command -v cargo >/dev/null 2>&1; then
    echo "✗ no cargo on PATH. This gate compiles; it does not skip."
    exit 1
fi

mapfile -t manifests < <(find "$SCAN_ROOT" -name Cargo.toml -not -path '*/target/*' | sort)

if [ "${#manifests[@]}" -eq 0 ]; then
    echo "✗ found no Cargo.toml under '$SCAN_ROOT' at all — the search is wrong, not the tree."
    exit 1
fi

# Only the standalone ones. A crate that is a workspace member is already compiled
# by `cargo check --workspace`, and checking it again here would just be slower.
standalone=()
for m in "${manifests[@]}"; do
    if grep -qE '^\[workspace\]' "$m"; then
        standalone+=("$m")
    fi
done

if [ "${#standalone[@]}" -eq 0 ]; then
    echo "✗ every Cargo.toml under '$SCAN_ROOT' is a workspace member."
    echo "    That may be true, but this gate exists for the standalone ones and would"
    echo "    now be checking nothing. Delete it, or fix the detection."
    exit 1
fi

echo "→ compiling ${#standalone[@]} standalone example crate(s) of ${#manifests[@]} found"

failed=0
for m in "${standalone[@]}"; do
    dir="$(dirname "$m")"
    echo "=== cargo check --all-targets $dir ==="
    if ! (cd "$dir" && cargo check --all-targets); then
        echo "✗ $dir does not compile."
        failed=1
    fi
done

if [ "$failed" -ne 0 ]; then
    echo
    echo "✗ a standalone example crate does not compile."
    echo "    Nothing else in this repository builds these, so a reader running"
    echo "    \`cargo run\` in one is the first to find out."
    exit 1
fi

echo "OK: all ${#standalone[@]} standalone example crate(s) compile, tests included."
