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

# ── assert_sdk_version_matches (H30 honesty gate) ──────────────────────────────

set +e
assert_sdk_version_matches "2.8.0" "2.8.0" "Python SDK" >/dev/null 2>&1
rc=$?
set -e
check "sdk-version-match: equal versions pass (rc 0)" "$rc" "0"

# The exact frozen state: the manifest stuck at 2.1.6 while a v2.8.0 tag is cut.
# This is the gate that was missing for four releases (H30).
set +e
assert_sdk_version_matches "2.1.6" "2.8.0" "Python SDK" >/dev/null 2>&1
rc=$?
set -e
check "sdk-version-match: frozen manifest fails the publish (rc 1)" "$rc" "1"

# ── bump_python_sdk_version ────────────────────────────────────────────────────

cat > "$WORK/pyproject.toml" <<'EOF'
[project]
name = "fraiseql"
version = "2.1.6"
dependencies = ["httpx>=0.27"]

[tool.uv]
dev-dependencies = ["pytest>=8.0"]
EOF
cat > "$WORK/__init__.py" <<'EOF'
"""FraiseQL."""
__version__ = "2.1.6"
EOF

bump_python_sdk_version 2.8.0 "$WORK/pyproject.toml" "$WORK/__init__.py"
check "bump-py: pyproject [project] version → 2.8.0" \
    "$(grep -c '^version = "2.8.0"' "$WORK/pyproject.toml")" "1"
check "bump-py: __version__ → 2.8.0" \
    "$(grep -c '^__version__ = "2.8.0"' "$WORK/__init__.py")" "1"
check "bump-py: dependency pins untouched" \
    "$(grep -c 'httpx>=0.27' "$WORK/pyproject.toml")" "1"

# ── bump_ts_sdk_version ─────────────────────────────────────────────────────────

cat > "$WORK/package.json" <<'EOF'
{
  "name": "fraiseql",
  "version": "2.1.6",
  "dependencies": {
    "zod": "^3.22.0"
  }
}
EOF
cat > "$WORK/package-lock.json" <<'EOF'
{
  "name": "fraiseql",
  "version": "2.1.6",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "fraiseql",
      "version": "2.1.6",
      "license": "MIT"
    },
    "node_modules/zod": {
      "version": "3.22.0"
    }
  }
}
EOF
cat > "$WORK/index.ts" <<'EOF'
export const version = "2.0.0-alpha.1";
EOF

bump_ts_sdk_version 2.8.0 "$WORK/package.json" "$WORK/package-lock.json" "$WORK/index.ts"
check "bump-ts: package.json version → 2.8.0" \
    "$(grep -c '"version": "2.8.0"' "$WORK/package.json")" "1"
check "bump-ts: lockfile bumps both package-own versions" \
    "$(grep -c '"version": "2.8.0"' "$WORK/package-lock.json")" "2"
check "bump-ts: lockfile dependency version (zod 3.22.0) untouched" \
    "$(grep -c '"version": "3.22.0"' "$WORK/package-lock.json")" "1"
check "bump-ts: index.ts version constant → 2.8.0 (L-ts-version)" \
    "$(grep -c '^export const version = "2.8.0"' "$WORK/index.ts")" "1"

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
if [[ "$TESTS_FAILED" -eq 0 ]]; then
    echo "release_helpers_test: all ${TESTS_RUN} checks passed."
else
    echo "release_helpers_test: ${TESTS_FAILED}/${TESTS_RUN} checks FAILED." >&2
    exit 1
fi
