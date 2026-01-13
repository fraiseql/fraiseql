#!/bin/bash

# fraiseql-wire Release Script
#
# This script automates the release process for fraiseql-wire.
#
# Usage: ./scripts/publish.sh <version>
# Example: ./scripts/publish.sh 0.2.0
#
# Prerequisites:
# - Git repo is clean (no uncommitted changes)
# - You have commit rights to main branch
# - You have push rights to GitHub
# - You have CARGO_TOKEN set for publishing to crates.io

set -e  # Exit on error

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
    exit 1
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

info() {
    echo -e "${YELLOW}→ $1${NC}"
}

# Check if version is provided
if [ -z "$1" ]; then
    error "Usage: $0 <version>"
fi

VERSION="$1"

# Validate version format (semver)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    error "Invalid version format: $VERSION (expected semver like 0.2.0)"
fi

info "Starting release process for version $VERSION"

# Check if on main branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$CURRENT_BRANCH" != "main" ]; then
    error "Must be on 'main' branch to release (currently on $CURRENT_BRANCH)"
fi

success "On main branch"

# Check if repo is clean
if ! git diff-index --quiet HEAD --; then
    error "Working directory has uncommitted changes. Commit or stash before releasing."
fi

success "Working directory is clean"

# Pull latest from origin
info "Pulling latest changes from origin"
git pull origin main

success "Repository is up to date"

# Update version in Cargo.toml
info "Updating version in Cargo.toml to $VERSION"
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

success "Updated Cargo.toml"

# Build in release mode to verify
info "Building in release mode"
cargo build --release
success "Build successful"

# Run tests
info "Running tests"
cargo test --lib
success "Tests passed"

# Run clippy
info "Running clippy"
cargo clippy -- -D warnings
success "Clippy checks passed"

# Check formatting
info "Checking code formatting"
cargo fmt -- --check
success "Code formatting is correct"

# Commit version bump
info "Committing version bump"
git add Cargo.toml
git commit -m "chore: bump version to $VERSION"
success "Committed version bump"

# Create git tag
info "Creating git tag v$VERSION"
git tag -a "v$VERSION" -m "Release version $VERSION

Changes in this release:
- See CHANGELOG.md for details

Published to: https://crates.io/crates/fraiseql-wire/versions"

success "Created git tag v$VERSION"

# Push to GitHub
info "Pushing to GitHub"
git push origin main
git push origin "v$VERSION"
success "Pushed to GitHub"

# Publish to crates.io
info "Publishing to crates.io"
cargo publish

success "Published to crates.io"

# Final summary
echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Release Complete! 🎉${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo ""
echo "Version: $VERSION"
echo "Git Tag: v$VERSION"
echo "Crates.io: https://crates.io/crates/fraiseql-wire/$VERSION"
echo "GitHub Release: https://github.com/fraiseql/fraiseql-wire/releases/tag/v$VERSION"
echo ""
echo "Next steps:"
echo "1. Visit GitHub Releases page to add release notes"
echo "2. Verify crates.io has the new version"
echo "3. Announce the release"
echo ""
