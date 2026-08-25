#!/usr/bin/env bash
# check-examples-compile.sh — every authoring artifact a shipped example carries must
# compile with the CLI in this tree.
#
# Background (Phase 09, #1052 and #1168): `examples/streaming/schema.json` is a JSON
# object keyed by type name where the compiler reads a sequence, so it has never been
# compilable by any version of this CLI; and `examples/async-jobs-subgraph/…/fraiseql.toml`
# sets `[database] pool_size`, which no version of the config schema accepts, so the file
# is refused at parse time. Both shipped for months. Nothing compiled them, because
# nothing in CI compiled any example at all.
#
# 25 example directories carry exactly six tracked authoring artifacts between them, so
# this gate is cheap. Its cost is one CLI binary, which is why it lives in a leg that has
# a Rust toolchain rather than in ShellGates.
#
# ⚠ `[domain_discovery]` resolves its root against the process's working directory, so
# each artifact is compiled FROM ITS OWN DIRECTORY. Compiling from the repo root silently
# discovers a different (or empty) domain and can pass on a file that a reader following
# the README cannot compile.
#
# ⚠ This gate never skips. A missing binary is a failure, not a pass: the whole point of
# the phase that produced it is that a check which quietly does nothing reads as green.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Accept a prebuilt binary (the CI leg builds one anyway); otherwise use whichever
# profile is on disk. Building here is deliberate: `cargo build` for a leg that already
# has the artifact is wasted minutes, and a silent `command -v fraiseql` could pick up an
# INSTALLED release from a different version — the one thing this gate must not measure.
FRAISEQL_BIN="${FRAISEQL_BIN:-}"
if [ -z "$FRAISEQL_BIN" ]; then
    for candidate in target/release/fraiseql target/debug/fraiseql; do
        if [ -x "$candidate" ]; then FRAISEQL_BIN="$REPO_ROOT/$candidate"; break; fi
    done
fi
if [ -z "$FRAISEQL_BIN" ] || [ ! -x "$FRAISEQL_BIN" ]; then
    echo "✗ no fraiseql binary to compile with."
    echo "    Build one:  cargo build -p fraiseql --bin fraiseql --features cli"
    echo "    Or point FRAISEQL_BIN at it. This gate does not skip."
    exit 1
fi
echo "→ compiling every example authoring artifact with $("$FRAISEQL_BIN" --version 2>/dev/null || echo "$FRAISEQL_BIN")"

# find, not `git ls-files`: the Dagger legs ignore `.git` and `git init` an empty repo,
# so an ls-files loop iterates over nothing and the gate cannot fail.
artifacts=$(find examples -name 'fraiseql.toml' -o -name 'schema.json' | sort)
if [ -z "$artifacts" ]; then
    echo "✗ found no example authoring artifacts at all — the search is wrong, not the tree."
    exit 1
fi

found=0
count=0
while IFS= read -r artifact; do
    [ -n "$artifact" ] || continue
    count=$((count + 1))
    dir="$(dirname "$artifact")"
    base="$(basename "$artifact")"
    if out=$(cd "$dir" && "$FRAISEQL_BIN" compile "$base" --check 2>&1); then
        echo "  ok   $artifact"
    else
        echo "  FAIL $artifact"
        printf '%s\n' "$out" | sed 's/^/         /'
        found=1
    fi
done <<< "$artifacts"

if [ "$found" -ne 0 ]; then
    echo
    echo "examples/ compile gate FAILED — an example ships an artifact the CLI cannot read."
    exit 1
fi
echo "OK: all $count example authoring artifacts compile."
