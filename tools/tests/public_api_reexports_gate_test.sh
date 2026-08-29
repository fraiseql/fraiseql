#!/usr/bin/env bash
# Self-test for tools/check-public-api-reexports.py — the red-capability pin for the
# #1198 gate.
#
# A gate that never fails is indistinguishable from a clean tree, and this one is
# especially easy to make vacuous: it discovers its subjects from the `cargo publish
# --package` steps in release.yml, so a rename there would silently leave it checking
# nothing. Every property below is asserted in BOTH directions.
#
# The fixtures are whole throwaway workspaces in a temp dir. The gate is never pointed
# at this repository with a mutation applied, so an interrupted run cannot leave the
# tree half-edited.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-public-api-reexports.py"

[ -f "$GATE" ] || { echo "❌ missing $GATE"; exit 1; }

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# fixture <id> <lib.rs body> [allowlist body] [release.yml override]
#
# Builds a one-crate workspace whose crate is published, with `serde_json` as its only
# third-party dependency. `cargo metadata --no-deps` reads the manifest without resolving
# or downloading anything, so these run offline.
fixture() {
    local id="$1" lib="$2" allow="${3:-}" publish="${4:-cargo publish --package fixture-crate}"
    local dir="$WORK/$id"
    mkdir -p "$dir/crates/fixture-crate/src" "$dir/tools" "$dir/.github/workflows"
    cat > "$dir/Cargo.toml" <<EOF
[workspace]
members = ["crates/fixture-crate"]
resolver = "2"
EOF
    cat > "$dir/crates/fixture-crate/Cargo.toml" <<EOF
[package]
name = "fixture-crate"
version = "0.1.0"
edition = "2021"

[dependencies]
serde_json = "1"
EOF
    printf '%s\n' "$lib" > "$dir/crates/fixture-crate/src/lib.rs"
    printf '%s\n' "$allow" > "$dir/tools/public-api-reexports.allow"
    printf 'jobs:\n  publish:\n    steps:\n      - run: %s\n' "$publish" \
        > "$dir/.github/workflows/release.yml"
    echo "$dir"
}

# fixture_unpublished <id>
#
# Two crates, only the clean one published. The crate with the unreachable reference is a
# workspace member release.yml does not name, so nobody outside can call it.
fixture_unpublished() {
    local id="$1"
    local dir
    dir="$(fixture "$id" 'pub fn config() -> serde_json::Value { serde_json::Value::Null }' '' \
           'cargo publish --package published-crate')"
    mkdir -p "$dir/crates/published-crate/src"
    cat > "$dir/crates/published-crate/Cargo.toml" <<EOF
[package]
name = "published-crate"
version = "0.1.0"
edition = "2021"
EOF
    echo 'pub fn ok() {}' > "$dir/crates/published-crate/src/lib.rs"
    sed -i 's|members = \["crates/fixture-crate"\]|members = ["crates/fixture-crate", "crates/published-crate"]|' \
        "$dir/Cargo.toml"
    echo "$dir"
}

# expect <pass|fail> <case-id> <description> <dir>
expect() {
    local want="$1" id="$2" desc="$3" dir="$4" got
    TESTS_RUN=$((TESTS_RUN + 1))
    if (cd "$dir" && python3 "$GATE" --root "$dir" >/dev/null 2>&1); then
        got=pass
    else
        got=fail
    fi
    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$desc" "$want" "$got"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

echo "public-api re-export gate self-test"
echo

echo "── a third-party type in a public signature needs a re-export ──"
expect fail A0 "public fn returning serde_json::Value, no re-export" \
    "$(fixture a0 'pub fn config() -> serde_json::Value { serde_json::Value::Null }')"
expect pass A1 "the same, with \`pub use serde_json;\`" \
    "$(fixture a1 'pub use serde_json;
pub fn config() -> serde_json::Value { serde_json::Value::Null }')"
expect pass A2 "the same, re-exported by type name" \
    "$(fixture a2 'pub use serde_json::Value;
pub fn config() -> serde_json::Value { serde_json::Value::Null }')"

echo
echo "── reachability, not mere presence ──"
# The item is `pub`, but nothing outside the crate can call it: `mod` is private, so
# the type never appears in the public API and the gate must stay quiet.
expect pass B0 "pub fn inside a private module" \
    "$(fixture b0 'mod hidden {
    pub fn config() -> serde_json::Value { serde_json::Value::Null }
}')"
expect fail B1 "the same module made \`pub mod\`" \
    "$(fixture b1 'pub mod hidden {
    pub fn config() -> serde_json::Value { serde_json::Value::Null }
}')"
# pub(crate) is not public API either — the distinction a naive `^\s*pub` match loses.
expect pass B2 "pub(crate) fn in a pub module" \
    "$(fixture b2 'pub mod hidden {
    pub(crate) fn config() -> serde_json::Value { serde_json::Value::Null }
}')"

echo
echo "── the allowlist works, and does not rot ──"
expect pass C0 "an exempted reference" \
    "$(fixture c0 'pub fn config() -> serde_json::Value { serde_json::Value::Null }' \
       'fixture-crate serde_json::Value it is exempt for this test')"
expect fail C1 "an exemption for a reference that is now reachable is stale" \
    "$(fixture c1 'pub use serde_json;
pub fn config() -> serde_json::Value { serde_json::Value::Null }' \
       'fixture-crate serde_json::Value it is exempt for this test')"

echo
echo "── the gate refuses to pass vacuously ──"
# If release.yml's publish steps are renamed away, the gate has no subjects. It must
# say so rather than report OK over an empty list — the shape #1206 shipped.
expect fail D0 "release.yml names no published crate" \
    "$(fixture d0 'pub fn config() -> serde_json::Value { serde_json::Value::Null }' '' \
       'echo nothing to publish')"
# An unpublished crate's public API is an internal seam; nobody outside can call it.
expect pass D1 "the offending crate is not published" \
    "$(fixture_unpublished d1)"

echo
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo "❌ $TESTS_FAILED of $TESTS_RUN assertions failed"
    exit 1
fi
echo "✅ all $TESTS_RUN assertions passed"
