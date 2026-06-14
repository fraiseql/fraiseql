#!/usr/bin/env bash
# FraiseQL release preparation script.
#
# Usage:
#   bash tools/release.sh <version>
#   make release VERSION=x.y.z
#
# Performs:
#   1. Validates VERSION as a semver string.
#   2. Bumps version in workspace Cargo.toml and all crate Cargo.toml files.
#   3. Updates CHANGELOG.md — promotes [Unreleased] to a versioned section.
#   4. Updates the version badge in README.md.
#   5. Commits all changes.
#   6. Creates an annotated git tag with the CHANGELOG notes as the message.
#
# The script is idempotent: re-running after a partial failure is safe because
# each step checks whether it is already done (grep / git status) before acting.
set -euo pipefail

# ── Helpers ─────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tools/lib/release_helpers.sh
source "$SCRIPT_DIR/lib/release_helpers.sh"

# ── Arguments ──────────────────────────────────────────────────────────────────

VERSION="${1:?Usage: $0 <version>    Example: $0 2.2.0}"

# ── Semver validation ──────────────────────────────────────────────────────────

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?$ ]]; then
    echo "ERROR: '$VERSION' is not a valid semver string." >&2
    echo "       Expected format: MAJOR.MINOR.PATCH[-prerelease][+build]" >&2
    exit 1
fi

# ── Guard: must be run from repository root ────────────────────────────────────

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [[ -z "$REPO_ROOT" ]]; then
    echo "ERROR: not inside a git repository." >&2
    exit 1
fi
cd "$REPO_ROOT"

echo "==> Preparing release v${VERSION}"
echo ""

# ── Step 1: Bump version in Cargo.toml files ──────────────────────────────────

echo "[1/7] Bumping version in Cargo.toml files..."

# Only update the workspace.package.version line in the root Cargo.toml
# to avoid accidental updates to dependency version pins.
if grep -q "^version = \"${VERSION}\"" Cargo.toml; then
    echo "      Already at version ${VERSION} in root Cargo.toml — skipping."
else
    # Update workspace.package.version
    sed -i "s/^version = \"[0-9][^\"]*\"/version = \"${VERSION}\"/" Cargo.toml
    echo "      Updated root Cargo.toml"
fi

# Bump the internal [workspace.dependencies] floors so a release that uses a
# brand-new cross-crate API doesn't resolve an older *published* sibling at
# `cargo publish --dry-run` time (the v2.4.0 cut compile-failed on exactly this).
# fraiseql-cli is left loose on purpose — see lib/release_helpers.sh.
bump_internal_dep_floors "$VERSION" Cargo.toml
echo "      Bumped internal dependency floors."

# Update [package] version in every standalone-versioned manifest (not
# [dependencies] lines). Members of the main workspace inherit the version via
# `version.workspace = true` and are bumped by the root edit above, so the awk
# below is a no-op for them. It only rewrites manifests that carry a literal
# `[package].version`: the 8 fuzz crates (their own workspaces) and the Rust
# SDK manifests under sdks/official/fraiseql-rust/ (also their own workspace).
while IFS= read -r crate_toml; do
    if grep -q "^version = \"${VERSION}\"" "$crate_toml"; then
        continue
    fi
    # Replace version = "x.y.z" only in the [package] section (first occurrence)
    # Using awk to limit replacement to the block before the first [dependencies]
    awk -v ver="$VERSION" '
        /^\[/ { in_package=0 }
        /^\[package\]/ { in_package=1 }
        in_package && /^version = / { sub(/"[^"]*"/, "\"" ver "\""); in_package=0 }
        { print }
    ' "$crate_toml" > "${crate_toml}.tmp" && mv "${crate_toml}.tmp" "$crate_toml"
done < <(find crates sdks/official/fraiseql-rust -name "Cargo.toml" -not -path "*/target/*")

# Bump the Python and TypeScript SDK manifests in lockstep with the crates.
# Without this the manifests stay frozen, the publish jobs build the stale
# version, and twine --skip-existing / npm "already published" silently no-op
# every release — the audit found v2.3.0–v2.6.0 SDKs never actually shipped (H30).
bump_python_sdk_version "$VERSION" \
    sdks/official/fraiseql-python/pyproject.toml \
    sdks/official/fraiseql-python/src/fraiseql/__init__.py
bump_ts_sdk_version "$VERSION" \
    sdks/official/fraiseql-typescript/package.json \
    sdks/official/fraiseql-typescript/package-lock.json \
    sdks/official/fraiseql-typescript/src/index.ts
echo "      Bumped Python + TypeScript SDK manifests."

echo "      Done."

# ── Step 2: Update CHANGELOG.md ───────────────────────────────────────────────

echo "[2/7] Updating CHANGELOG.md..."

CHANGELOG="CHANGELOG.md"
DATE=$(date +%Y-%m-%d)
VERSIONED_HEADER="## [${VERSION}] - ${DATE}"

if grep -qF "$VERSIONED_HEADER" "$CHANGELOG"; then
    echo "      ${VERSIONED_HEADER} already present — skipping."
else
    # Insert versioned section after the [Unreleased] line
    # Produces:  ## [Unreleased]
    #            (blank line)
    #            ## [x.y.z] - YYYY-MM-DD
    sed -i "s/^## \[Unreleased\]/## [Unreleased]\n\n${VERSIONED_HEADER}/" "$CHANGELOG"
    echo "      Added ${VERSIONED_HEADER} to CHANGELOG.md"
fi

# ── Step 3: Update README.md version badge ────────────────────────────────────

echo "[3/7] Updating README.md version badge..."

README="README.md"
if grep -qF "v${VERSION}" "$README"; then
    echo "      README.md badge already at v${VERSION} — skipping."
else
    # Replace version strings in badge URLs (shields.io badge format)
    sed -i "s/v[0-9]\+\.[0-9]\+\.[0-9]\+\(-[a-zA-Z0-9.]*\)\?/v${VERSION}/g" "$README"
    echo "      Updated version references in README.md"
fi

# ── Step 4: cargo check to validate version bump didn't break anything ─────────

echo "[4/7] Running cargo check..."
cargo check --workspace --quiet
echo "      cargo check passed."

# ── Step 5: Git commit ────────────────────────────────────────────────────────

echo "[5/7] Committing..."
COMMIT_MSG="chore(release): prepare v${VERSION}"

# Stage every manifest the bump step can touch: the root, all workspace
# members, the 8 fuzz crates, and the Rust SDK manifests — plus Cargo.lock,
# CHANGELOG, and README. The fuzz/SDK globs were previously omitted, which left
# their bumped versions unstaged and forced a manual `git add` each release.
RELEASE_FILES=(
    Cargo.toml
    crates/*/Cargo.toml
    crates/*/fuzz/Cargo.toml
    sdks/official/fraiseql-rust/Cargo.toml
    sdks/official/fraiseql-rust/*/Cargo.toml
    sdks/official/fraiseql-python/pyproject.toml
    sdks/official/fraiseql-python/src/fraiseql/__init__.py
    sdks/official/fraiseql-typescript/package.json
    sdks/official/fraiseql-typescript/package-lock.json
    sdks/official/fraiseql-typescript/src/index.ts
    "$CHANGELOG"
    "$README"
)

if git diff --cached --quiet && git diff --quiet -- "${RELEASE_FILES[@]}"; then
    echo "      Nothing to commit — release files already up to date."
else
    git add Cargo.lock "${RELEASE_FILES[@]}"
    git commit -m "$COMMIT_MSG"
    echo "      Committed: ${COMMIT_MSG}"
fi

# ── Step 6: Pre-tag release validation (Dagger; read-only, never publishes) ────

echo "[6/7] Pre-tag release validation..."

if [[ -n "${SKIP_RELEASE_VALIDATE:-}" ]]; then
    echo "      SKIP_RELEASE_VALIDATE set — skipping."
    echo "      Run 'make release-validate VERSION=${VERSION}' before pushing the tag."
elif ! command -v dagger >/dev/null 2>&1; then
    echo "      WARNING: 'dagger' not on PATH — cannot run the pre-tag validation gate."
    echo "      WARNING: Run 'make release-validate VERSION=${VERSION}' on a machine with"
    echo "      WARNING: Dagger BEFORE pushing the v${VERSION} tag. It catches the publish-order,"
    echo "      WARNING: unpublished-sibling, and dry-run-compile failures that have repeatedly"
    echo "      WARNING: broken releases only after the tag was already pushed."
else
    echo "      Running 'make release-validate VERSION=${VERSION}' (publish-order self-test + dry-run)..."
    if make release-validate VERSION="${VERSION}"; then
        echo "      Pre-tag validation passed."
    else
        echo "ERROR: pre-tag release validation FAILED — not creating the tag." >&2
        echo "       Fix the issues reported above, then re-run this script." >&2
        exit 1
    fi
fi

# ── Step 7: Annotated git tag ─────────────────────────────────────────────────

echo "[7/7] Creating annotated tag v${VERSION}..."

if git rev-parse "v${VERSION}" >/dev/null 2>&1; then
    echo "      Tag v${VERSION} already exists — skipping."
else
    # Extract the changelog notes for this version (the lines after the version
    # header, up to the next ## section) as the tag message.
    NOTES=$(extract_changelog_notes "$VERSION" "$CHANGELOG")
    TAG_MSG="Release v${VERSION}

${NOTES}"
    git tag -a "v${VERSION}" -m "$TAG_MSG"
    echo "      Created tag v${VERSION}"
fi

# ── Done ──────────────────────────────────────────────────────────────────────

echo ""
echo "Release v${VERSION} prepared."
echo ""
echo "Next steps:"
echo "  git push origin $(git branch --show-current)"
echo "  git push origin v${VERSION}"
