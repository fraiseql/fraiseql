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

# Compute the cargo sparse-index URL for a crate name.
#
# `cargo publish` resolves dependency versions from the SPARSE INDEX
# (index.crates.io), NOT the crates.io API — the v2.5.0 cut hit a partial publish
# because the tier-wait polled only the API (200 once the web DB had the row) while
# the index, which lags by tens of seconds, had not yet advertised the version, so
# the next tier's `cargo publish` failed with "failed to select a version". The
# path prefix follows the registry index spec, keyed on the (lowercased) name
# length:
#   1 char  -> 1/<name>
#   2 chars -> 2/<name>
#   3 chars -> 3/<first-char>/<name>
#   >=4     -> <first-2>/<next-2>/<name>   (all fraiseql crates land here: fr/ai/…)
#
# Usage: index_url_for <crate>
index_url_for() {
    local crate="$1" lower len prefix
    lower="$(printf '%s' "$crate" | tr '[:upper:]' '[:lower:]')"
    len=${#lower}
    case "$len" in
        1) prefix="1" ;;
        2) prefix="2" ;;
        3) prefix="3/${lower:0:1}" ;;
        *) prefix="${lower:0:2}/${lower:2:2}" ;;
    esac
    printf 'https://index.crates.io/%s/%s' "$prefix" "$lower"
}

# Report (exit 0) whether a sparse-index response body advertises an exact version.
#
# The index returns newline-delimited JSON, one compact record per published
# version, each carrying `"vers":"X.Y.Z"` (no spaces). Matching that whole token as
# a FIXED string is what makes the check exact: a bare `2.5.0` would match inside
# `12.5.0` or `2.5.01`, but `"vers":"2.5.0"` (with the closing quote) cannot. Yanked
# status is irrelevant — a freshly published version is present and non-yanked, and
# cargo only needs the version to exist in the index.
#
# Usage: index_body_has_version "<body>" <version>
index_body_has_version() {
    local body="$1" version="$2"
    printf '%s' "$body" | grep -qF "\"vers\":\"$version\""
}

# Bump the version in a Python SDK: the [project].version in pyproject.toml and
# the __version__ constant in the package __init__.py. Both edits are anchored to
# line-start so only the package's own version is rewritten — dependency pins
# (`httpx>=0.27`, etc.) live elsewhere and are never line-anchored this way.
#
# Without this bump tools/release.sh leaves the SDK manifests frozen; the publish
# job then builds the stale version and twine --skip-existing silently no-ops it
# (audit H30 — v2.3.0–v2.6.0 Python SDK publishes never actually shipped).
#
# Usage: bump_python_sdk_version <version> <pyproject.toml> <__init__.py>
bump_python_sdk_version() {
    local version="$1" pyproject="$2" init_py="$3"
    sed -i -E "s/^version = \"[0-9][^\"]*\"/version = \"${version}\"/" "$pyproject"
    sed -i -E "s/^__version__ = \"[0-9][^\"]*\"/__version__ = \"${version}\"/" "$init_py"
}

# Bump the version in the TypeScript SDK: package.json, the two package-own
# "version" fields in package-lock.json (root + packages[""], both within the
# first dozen lines), and the exported `version` constant in src/index.ts.
#
# The lockfile edit is confined to lines 1-12 so the package's own versions are
# rewritten while every dependency version deeper in the file is left intact.
# Bumping the index.ts constant also fixes the stale "2.0.0-alpha.1" it had
# drifted to (audit L-ts-version).
#
# Usage: bump_ts_sdk_version <version> <package.json> <package-lock.json> <index.ts>
bump_ts_sdk_version() {
    local version="$1" pkg="$2" lock="$3" index_ts="$4"
    # package.json: the top-level "version" is the first such key in the file.
    sed -i -E "0,/\"version\": \"[0-9][^\"]*\"/s//\"version\": \"${version}\"/" "$pkg"
    # package-lock.json: only the package's own version fields live in lines 1-12.
    sed -i -E "1,12 s/\"version\": \"[0-9][^\"]*\"/\"version\": \"${version}\"/" "$lock"
    # index.ts exported constant (fixes L-ts-version).
    sed -i -E "s/^export const version = \"[^\"]*\"/export const version = \"${version}\"/" "$index_ts"
}

# Honesty gate for the SDK publish jobs: refuse to publish when the SDK manifest
# version does not match the release version being published. This is the exact
# frozen state — the manifest stuck at 2.1.6 while v2.3.0–v2.6.0 tags were cut —
# that silently no-oped four SDK releases behind green checkmarks (audit H30).
# Prints a diagnostic and returns 1 on mismatch; returns 0 (with a confirmation)
# when they match.
#
# Usage: assert_sdk_version_matches <manifest_version> <release_version> [label]
assert_sdk_version_matches() {
    local manifest="$1" release="$2" label="${3:-SDK}"
    if [[ "$manifest" != "$release" ]]; then
        echo "ERROR: ${label} manifest version '${manifest}' does not match release version '${release}'." >&2
        echo "       The release tag is v${release} but the ${label} manifest was never bumped to it." >&2
        echo "       Refusing to publish — this is the frozen-SDK state that silently no-oped" >&2
        echo "       v2.3.0–v2.6.0 SDK publishes behind green checkmarks (audit H30)." >&2
        echo "       Re-cut the release with 'make release VERSION=${release}' so tools/release.sh" >&2
        echo "       bumps the SDK manifests in lockstep with the crates." >&2
        return 1
    fi
    echo "OK: ${label} manifest is at the release version ${release}."
}
