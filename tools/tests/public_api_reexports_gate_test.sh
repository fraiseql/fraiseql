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
#
# ⚠ A fixture must differ under the gate it pins and the revision that could not see it.
# The first ten cases here were thorough about properties and uniform about SPELLING —
# every one wrote `serde_json::Value` in full — so all ten went through the qualified-path
# branch while the imported-name branch matched nothing in the entire workspace (#1234).
# Two of the replacement cases had the same fault on the first draft: a fixture whose
# lib.rs BEGINS with `use serde_json::Value;` passes under the broken gate too, because
# byte 0 is exactly where its `^` did match. Open fixtures with a doc comment, as real
# files do, and check each new case against the older copy before trusting it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# PUBLIC_API_GATE points the suite at another copy of the gate. That is how each
# case below is shown to be RED-capable: run it against the previous revision of
# the gate and the cases the revision cannot see must fail.
GATE="${PUBLIC_API_GATE:-$REPO_ROOT/tools/check-public-api-reexports.py}"

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
thiserror = "2"
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
echo "── a type named by its import, not by its path ──"
# `use serde_json::Value; -> Value` is the ordinary Rust idiom and was the gate's blind
# spot: USE_STMT was compiled without re.M, so `^` anchored at byte 0 of the file and
# matched a `use` only in a file that opens with one. Every case above spells the type
# `serde_json::Value` in the signature, so all ten passed through the one branch that
# worked while the imported-name branch matched nothing in the entire workspace.
expect fail E0 "public fn returning an imported \`Value\`, no re-export" \
    "$(fixture e0 '//! A crate root opens with its doc comment, not with a `use`.
use serde_json::Value;
pub fn config() -> Value { Value::Null }')"
expect pass E1 "the same, with \`pub use serde_json;\`" \
    "$(fixture e1 '//! A crate root opens with its doc comment, not with a `use`.
use serde_json::Value;
pub use serde_json;
pub fn config() -> Value { Value::Null }')"
# A qualified path says which crate it comes from. Reading it as a bare name too would
# attribute it to whichever crate the file imports a same-named type from — thiserror,
# in every file that derives an error.
expect pass E2 "a qualified type is attributed to its path, not to a same-named import" \
    "$(fixture e2 '//! A crate root opens with its doc comment, not with a `use`.
use thiserror::Error;
pub use serde_json;
#[derive(Debug, Error)]
pub enum Wrapped { #[error("x")] X }
pub fn parse() -> Result<serde_json::Value, serde_json::Error> { todo!() }')"

echo
echo "── an enum variant carries types the same way a field does ──"
# `signatures()` reads a declaration only as far as its opening brace, so a whole enum
# body was invisible. A downstream that matches on the enum, or constructs one, has to
# name whatever the variants carry.
expect fail F0 "a tuple variant carrying serde_json::Value" \
    "$(fixture f0 'pub enum Handled {
    Recorded(serde_json::Value),
    Duplicate,
}')"
expect pass F1 "the same, with \`pub use serde_json;\`" \
    "$(fixture f1 'pub use serde_json;
pub enum Handled {
    Recorded(serde_json::Value),
    Duplicate,
}')"
expect fail F2 "a struct variant whose field carries serde_json::Value" \
    "$(fixture f2 'pub enum Handled {
    Failed { body: serde_json::Value },
    Duplicate,
}')"
# Prose in a variant\'s doc comment is prose. Matching it reported a dependency for a
# type the enum does not mention.
expect pass F3 "a doc comment naming a type word" \
    "$(fixture f3 '//! `Error` is imported here and re-exported nowhere, and the enum names it
//! only in prose. A body scan that reads doc comments reports it anyway.
use thiserror::Error;
pub enum Handled {
    /// Error is reported to the caller, which decides what to do about it.
    Duplicate,
}')"
# A unit variant named after an imported type is the enum naming itself.
expect pass F4 "a unit variant whose identifier collides with an import" \
    "$(fixture f4 'use serde_json::Value;
pub enum Kind { Value, Other }
pub fn kind() -> Kind { Kind::Other }')"

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
