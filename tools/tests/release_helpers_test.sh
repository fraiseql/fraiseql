#!/usr/bin/env bash
# Unit tests for tools/lib/release_helpers.sh.
#
# Run directly:  bash tools/tests/release_helpers_test.sh
# Exits non-zero if any assertion fails, so it can be wired into a gate.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# shellcheck source=tools/lib/release_helpers.sh
source "$REPO_ROOT/tools/lib/release_helpers.sh"

TESTS_RUN=0
TESTS_FAILED=0

check() { # <name> <actual> <expected>
    TESTS_RUN=$((TESTS_RUN + 1))
    if [[ "$2" == "$3" ]]; then
        echo "  ok: $1"
    else
        echo "  FAIL: $1 — expected [$3], got [$2]" >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ── extract_changelog_notes ────────────────────────────────────────────────────

cat > "$WORK/CHANGELOG_short.md" <<'EOF'
# Changelog

## [Unreleased]

## [2.5.0] - 2026-06-05
### Added
- Feature A
- Feature B

## [2.4.0] - 2026-06-05
### Added
- Old feature
EOF

notes="$(extract_changelog_notes 2.5.0 "$WORK/CHANGELOG_short.md")"
check "notes/short: includes the version body"        "$(printf '%s\n' "$notes" | grep -c 'Feature A')"        "1"
check "notes/short: stops at the next version"         "$(printf '%s\n' "$notes" | grep -c 'Old feature')"      "0"
check "notes/short: strips the version header line"    "$(printf '%s\n' "$notes" | grep -c '## \[2.5.0\]')"     "0"

# A section longer than the 50-line cap must NOT abort under `set -o pipefail`
# (the SIGPIPE-from-head bug that forced manual tagging on every v2.4.0 attempt).
{
    echo "# Changelog"; echo; echo "## [Unreleased]"; echo
    echo "## [2.5.0] - 2026-06-05"
    for i in $(seq 1 80); do echo "- entry line $i"; done
    echo; echo "## [2.4.0] - 2026-06-05"; echo "- old"
} > "$WORK/CHANGELOG_long.md"

set +e
notes_long="$(extract_changelog_notes 2.5.0 "$WORK/CHANGELOG_long.md")"
rc=$?
set -e
check "notes/long: extraction exits 0 (no SIGPIPE abort)" "$rc" "0"
check "notes/long: truncated to the 50-line cap"          "$(printf '%s\n' "$notes_long" | wc -l | tr -d ' ')" "50"

# ── bump_internal_dep_floors ───────────────────────────────────────────────────

cat > "$WORK/Cargo.toml" <<'EOF'
[workspace.package]
version = "2.4.0"

[workspace.dependencies]
axum = {version = "0.8"}
fraiseql-cli = {path = "crates/fraiseql-cli", version = "2.3.0"}
fraiseql-core = {path = "crates/fraiseql-core", version = "2.4.0", default-features = false}
fraiseql-db = {path = "crates/fraiseql-db", version = "2.4.0", default-features = false}
serde = {version = "1.0", features = ["derive"]}

[some.other.table]
version = "2.4.0"
EOF

bump_internal_dep_floors 2.5.0 "$WORK/Cargo.toml"

check "bump: fraiseql-core floor → 2.5.0" \
    "$(grep -c 'fraiseql-core = {path = "crates/fraiseql-core", version = "2.5.0", default-features = false}' "$WORK/Cargo.toml")" "1"
check "bump: fraiseql-db floor → 2.5.0" \
    "$(grep -c 'fraiseql-db = {path = "crates/fraiseql-db", version = "2.5.0", default-features = false}' "$WORK/Cargo.toml")" "1"
check "bump: fraiseql-cli floor stays loose at 2.3.0" \
    "$(grep -c 'fraiseql-cli = {path = "crates/fraiseql-cli", version = "2.3.0"}' "$WORK/Cargo.toml")" "1"
check "bump: external deps (axum) untouched" \
    "$(grep -c 'axum = {version = "0.8"}' "$WORK/Cargo.toml")" "1"
check "bump: version lines outside [workspace.dependencies] untouched" \
    "$(grep -c '^version = "2.4.0"' "$WORK/Cargo.toml")" "2"

# Idempotent: a second run on the already-bumped file is a no-op.
before="$(cat "$WORK/Cargo.toml")"
bump_internal_dep_floors 2.5.0 "$WORK/Cargo.toml"
after="$(cat "$WORK/Cargo.toml")"
check "bump: idempotent on a second run" "$after" "$before"

# ── index_url_for ───────────────────────────────────────────────────────────────

check "index_url: >=4 chars -> first2/next2"  "$(index_url_for fraiseql-wire)" "https://index.crates.io/fr/ai/fraiseql-wire"
check "index_url: root crate fraiseql"        "$(index_url_for fraiseql)"      "https://index.crates.io/fr/ai/fraiseql"
check "index_url: 1-char name"                "$(index_url_for a)"             "https://index.crates.io/1/a"
check "index_url: 2-char name"                "$(index_url_for ab)"            "https://index.crates.io/2/ab"
check "index_url: 3-char name"                "$(index_url_for abc)"           "https://index.crates.io/3/a/abc"
check "index_url: lowercases the name"        "$(index_url_for Fraiseql-CLI)"  "https://index.crates.io/fr/ai/fraiseql-cli"

# ── index_body_has_version ──────────────────────────────────────────────────────

INDEX_BODY='{"name":"fraiseql-wire","vers":"2.4.0","yanked":false}
{"name":"fraiseql-wire","vers":"2.5.0","yanked":false}'

if index_body_has_version "$INDEX_BODY" "2.5.0"; then r=yes; else r=no; fi
check "index_has_version: present version -> yes" "$r" "yes"

if index_body_has_version "$INDEX_BODY" "2.6.0"; then r=yes; else r=no; fi
check "index_has_version: absent version -> no" "$r" "no"

# A shorter version must NOT match as a substring of a longer one — the exact
# `"vers":"X.Y.Z"` token (closing quote included) is what prevents that.
SUBSTR_BODY='{"name":"x","vers":"12.5.0","yanked":false}
{"name":"x","vers":"2.5.01","yanked":false}'
if index_body_has_version "$SUBSTR_BODY" "2.5.0"; then r=yes; else r=no; fi
check "index_has_version: no false substring match (12.5.0 / 2.5.01)" "$r" "no"

# A yanked record still counts as present (cargo only needs the version to exist).
YANKED_BODY='{"name":"fraiseql","vers":"2.5.0","yanked":true}'
if index_body_has_version "$YANKED_BODY" "2.5.0"; then r=yes; else r=no; fi
check "index_has_version: yanked record still present -> yes" "$r" "yes"

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
if [[ "$TESTS_FAILED" -eq 0 ]]; then
    echo "release_helpers_test: all ${TESTS_RUN} checks passed."
else
    echo "release_helpers_test: ${TESTS_FAILED}/${TESTS_RUN} checks FAILED." >&2
    exit 1
fi
