#!/usr/bin/env bash
# Shared, unit-testable helpers for tools/release.sh.
#
# Sourced by tools/release.sh and by tools/tests/release_helpers_test.sh. Keep
# every function pure — operate only on the arguments passed in — so the tests
# can exercise them against fixtures without running the whole release flow.

# Extract the CHANGELOG notes for a version, for use as a git tag message.
# Prints the lines after the version's `## [x.y.z]` header up to (but not
# including) the next `## [` section, with blank lines stripped, capped at 50
# lines.
#
# Two release-tag bugs are fixed here together. The original used an awk range
# `/^## \[x.y.z\]/,/^## \[/`, but the end pattern `^## \[` also matches the
# header line that opened the range, so the range was a single line — the notes
# came out empty and the tag had to be written by hand. This awk instead skips
# the header and prints until the next section. And `head -50` closes the pipe
# once it has its lines, sending SIGPIPE upstream; under `set -o pipefail` (which
# release.sh sets) that surfaced as a non-zero status and `set -e` aborted the
# release just before tagging. The pipeline runs in a subshell with pipefail
# disabled so the status is `head`'s (always 0), regardless of section length,
# while leaving the caller's shell options untouched.
#
# Usage: extract_changelog_notes <version> <changelog-file>
extract_changelog_notes() {
    local version="$1" changelog="$2"
    (
        set +o pipefail
        awk -v ver="$version" '
            $0 ~ "^## \\[" ver "\\]" { found = 1; next }
            found && /^## \[/        { exit }
            found                    { print }
        ' "$changelog" \
            | sed '/^[[:space:]]*$/d' \
            | head -50
    )
}

# Bump the [workspace.dependencies] floors of the internal fraiseql-* crates to
# the release version. Without this, a release that uses a brand-new cross-crate
# API leaves the sibling floors at the previous version, so `cargo publish
# --dry-run` resolves an older *published* sibling and compile-fails — the
# v2.4.0 cut hit exactly this (core@2.4.0 floored db at ^2.3.0, dry-run resolved
# the published db 2.3.2 which lacked the new method).
#
# fraiseql-cli is deliberately skipped: fraiseql-server carries fraiseql-cli as a
# [dev-dependency] while fraiseql-cli depends on fraiseql-server (a dev cycle), so
# cli's floor must stay loose (at an already-published version) or `cargo publish`
# of fraiseql-server cannot resolve its cli dev-dep against an unpublished version.
#
# Only the [workspace.dependencies] table is touched, and only entries that are
# internal path deps (`path = "crates/..."`); external deps and version lines in
# other tables are left alone. The function is idempotent.
#
# Usage: bump_internal_dep_floors <version> <cargo-toml-file>
bump_internal_dep_floors() {
    local version="$1" cargo_toml="$2"
    awk -v ver="$version" '
        /^\[/ { in_deps = ($0 == "[workspace.dependencies]") }
        in_deps && /^fraiseql-/ && /path = "crates\// && $0 !~ /^fraiseql-cli[ =]/ {
            sub(/version = "[0-9][^"]*"/, "version = \"" ver "\"")
        }
        { print }
    ' "$cargo_toml" > "${cargo_toml}.tmp" && mv "${cargo_toml}.tmp" "$cargo_toml"
}
