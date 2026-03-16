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

echo "[1/6] Bumping version in Cargo.toml files..."

# Only update the workspace.package.version line in the root Cargo.toml
# to avoid accidental updates to dependency version pins.
if grep -q "^version = \"${VERSION}\"" Cargo.toml; then
    echo "      Already at version ${VERSION} in root Cargo.toml — skipping."
else
    # Update workspace.package.version
    sed -i "s/^version = \"[0-9][^\"]*\"/version = \"${VERSION}\"/" Cargo.toml
    echo "      Updated root Cargo.toml"
fi

# Update [package] version in each crate's Cargo.toml (not [dependencies] lines)
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
done < <(find crates -name "Cargo.toml" -not -path "*/target/*")

echo "      Done."

# ── Step 2: Update CHANGELOG.md ───────────────────────────────────────────────

echo "[2/6] Updating CHANGELOG.md..."

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

echo "[3/6] Updating README.md version badge..."

README="README.md"
if grep -qF "v${VERSION}" "$README"; then
    echo "      README.md badge already at v${VERSION} — skipping."
else
    # Replace version strings in badge URLs (shields.io badge format)
    sed -i "s/v[0-9]\+\.[0-9]\+\.[0-9]\+\(-[a-zA-Z0-9.]*\)\?/v${VERSION}/g" "$README"
    echo "      Updated version references in README.md"
fi

# ── Step 4: cargo check to validate version bump didn't break anything ─────────

echo "[4/6] Running cargo check..."
cargo check --workspace --quiet
echo "      cargo check passed."

# ── Step 5: Git commit ────────────────────────────────────────────────────────

echo "[5/6] Committing..."
COMMIT_MSG="chore(release): prepare v${VERSION}"

if git diff --cached --quiet && git diff --quiet -- Cargo.toml crates/*/Cargo.toml "$CHANGELOG" "$README"; then
    echo "      Nothing to commit — release files already up to date."
else
    git add Cargo.toml Cargo.lock crates/*/Cargo.toml "$CHANGELOG" "$README"
    git commit -m "$COMMIT_MSG"
    echo "      Committed: ${COMMIT_MSG}"
fi

# ── Step 6: Annotated git tag ─────────────────────────────────────────────────

echo "[6/6] Creating annotated tag v${VERSION}..."

if git rev-parse "v${VERSION}" >/dev/null 2>&1; then
    echo "      Tag v${VERSION} already exists — skipping."
else
    # Extract the changelog notes for this version (between the version header
    # and the next ## section) as the tag message.
    NOTES=$(awk "/^## \[${VERSION}\]/,/^## \[/" "$CHANGELOG" \
        | grep -v "^## \[" \
        | sed '/^[[:space:]]*$/d' \
        | head -50)
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
