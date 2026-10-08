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

# Bump the version a TOML lockfile (uv.lock, Cargo.lock) records for one package: the
# `version` line of the `[[package]]` block whose `name` is exactly <package>.
#
# The SDK manifests were bumped while their lockfiles kept the old version, so
# `uv sync --locked` and `cargo test --locked` refused the release commit (#1225).
# tools/check-sdk-lockfile-freshness.py is the gate. Only the package's own record
# moves: dependency entries, a namesake-prefixed package and the lock format's own
# `version` are left as they are. Fails when no record names the package.
#
# Usage: bump_lockfile_package_version <version> <lockfile> <package>
bump_lockfile_package_version() {
    local version="$1" lock="$2" package="$3"
    awk -v ver="$version" -v pkg="$package" '
        /^\[\[package\]\]$/       { block = 1; named = 0; print; next }
        /^$/                       { block = 0; named = 0 }
        block && $0 == "name = \"" pkg "\"" { named = 1; print; next }
        named && /^version = "/    { sub(/"[^"]*"/, "\"" ver "\""); named = 0; bumped++ }
        { print }
        END { exit (bumped == 1 ? 0 : 1) }
    ' "$lock" > "${lock}.tmp" || {
        rm -f "${lock}.tmp"
        echo "ERROR: ${lock} has no single [[package]] record named \"${package}\"." >&2
        return 1
    }
    mv "${lock}.tmp" "$lock"
}

# Bump the version a Cargo.lock records for every PATH package: each `[[package]]`
# block with no `source` line, i.e. a crate built from a manifest in this repository.
#
# For the lockfiles of workspaces whose every path package moves with the release —
# the fuzz workspaces and the Rust SDK's nested fraiseql-client. The 2.16.0 cut bumped
# their manifests and left these records at 2.15.0 (fraiseql-client's at 2.3.0); no
# build of them runs `--locked`, so nothing noticed until
# tools/check-cargo-lock-path-versions.py. Cargo writes `source` after `version`, so
# each block is buffered and rewritten only once it is known to have no source.
# Registry and git records, and the lock format's own `version`, are left as they are.
# Fails when the lockfile holds no path package.
#
# Usage: bump_lockfile_path_packages <version> <lockfile>
bump_lockfile_path_packages() {
    local version="$1" lock="$2"
    awk -v ver="$version" '
        function flush(   i) {
            for (i = 1; i <= n; i++) {
                if (!sourced && i == vline) { sub(/"[^"]*"/, "\"" ver "\"", buf[i]); bumped++ }
                print buf[i]
            }
            n = 0; vline = 0; sourced = 0; block = 0
        }
        /^\[\[package\]\]$/ { flush(); block = 1 }
        block && /^$/       { flush(); print; next }
        block {
            buf[++n] = $0
            if ($0 ~ /^version = "/) vline = n
            if ($0 ~ /^source = /)   sourced = 1
            next
        }
        { print }
        END { flush(); exit (bumped > 0 ? 0 : 1) }
    ' "$lock" > "${lock}.tmp" || {
        rm -f "${lock}.tmp"
        echo "ERROR: ${lock} records no path package." >&2
        return 1
    }
    mv "${lock}.tmp" "$lock"
}

# Bump the version strings in the shipped deployment artifacts: the Dockerfile's OCI
# version label, the Helm chart's `version` + `appVersion` (lockstep, see Chart.yaml's
# header), and values.yaml's `image.tag`.
#
# Without this, tools/release.sh bumps the crates and the SDKs and leaves the deploy
# artifacts frozen — which is exactly what happened: the OCI label sat at 2.1.1 and the
# chart at 2.1.1/2.1.0 while the product shipped 2.14.1, and `helm install` on the
# defaults pulled `docker.io/library/fraiseql:2.8.0`, an image that does not exist
# (#1129). tools/check-deploy-versions.sh is the gate that keeps this honest.
#
# ⚠ Every substitution is anchored. `Dockerfile:8` is `FROM rust:1.95.0-slim`, and a
# blanket version-shaped rewrite would silently move the toolchain pin — a different
# concern, owned by #1107. Likewise `appVersion:` must not be caught by the `version:`
# pattern, which is why the chart edits anchor at the start of the line.
#
# Usage: bump_deploy_artifacts <version> <Dockerfile> <Chart.yaml> <values.yaml> [image-pin file...]
bump_deploy_artifacts() {
    local version="$1" dockerfile="$2" chart="$3" values="$4"
    shift 4
    # OCI label only — keyed on the label name, never on a bare version shape.
    sed -i -E "s|org\.opencontainers\.image\.version=\"[^\"]*\"|org.opencontainers.image.version=\"${version}\"|" "$dockerfile"
    # Chart: `^version:` and `^appVersion:` are distinct anchors; appVersion keeps its quotes.
    sed -i -E "s/^version: .*/version: ${version}/" "$chart"
    sed -i -E "s/^appVersion: .*/appVersion: \"${version}\"/" "$chart"
    # values.yaml: the indented `tag:` under `image:`. The file has exactly one.
    sed -i -E "s/^( *)tag: \"[^\"]*\"/\1tag: \"${version}\"/" "$values"
    # The remaining files pin a published server image: the Compose stack and the
    # runbooks. Keyed on the image name (`fraiseql/server`, `-full`, `-platform`, with or
    # without the ghcr.io registry) and on a runbook's `IMAGE_TAG=` assignment; a version
    # anywhere else in the file (another image, prose) is left alone.
    local file
    for file in "$@"; do
        sed -i -E \
            -e "s#((ghcr\.io/)?fraiseql/server(-full|-platform)?:)[0-9]+\.[0-9]+\.[0-9]+[^[:space:]\"']*#\1${version}#g" \
            -e "s#^IMAGE_TAG=[0-9][^[:space:]]*#IMAGE_TAG=${version}#" \
            "$file"
    done
}

# Restamp the compiled schemas CI boots with the version being released.
#
# Since #1304 the server refuses any compiled schema its own build did not produce, and
# `release-smoke.yml` boots `docker/e2e/*.compiled.json`. Without this, the release commit
# leaves those artifacts naming the PREVIOUS release, and the first witness is the tag —
# where the smoke server refuses to start. tools/check-compiled-schema-stamp.sh is the
# gate that fails on the release branch if this call is ever removed.
#
# ⚠ Anchored on the `"fraiseql_version"` key, never on a bare version shape: these files
# are compiled schemas and may legitimately carry other version-shaped strings (an
# `api_version`, a semver in a description). A missing file is skipped rather than fatal,
# as with the doc status lines — a rename must not abort a release mid-bump.
#
# Usage: bump_compiled_schema_stamps <version> <file...>
bump_compiled_schema_stamps() {
    local version="$1"
    shift
    local f
    for f in "$@"; do
        [ -f "$f" ] || continue
        sed -i -E "s/(\"fraiseql_version\"[[:space:]]*:[[:space:]]*)\"[^\"]*\"/\1\"${version}\"/" "$f"
    done
}

# Rewrite the `vX.Y.Z released` status lines in docs/ that tools/check-docs-version.sh
# enforces.
#
# Without this, `make release VERSION=<n>` bumps Cargo.toml and leaves those lines naming
# the PREVIOUS release — and check-docs-version.sh runs in ShellGates, which is part of the
# REQUIRED preflight check. So the release commit itself turned a required gate red, and the
# release branch could not pass CI until someone edited four files by hand (#1134). The gate
# had been green for a year only because docs and Cargo.toml had only ever been edited
# together by hand; the first automated bump is what broke it.
#
# ⚠ Deliberately narrow. check-docs-version.sh's own header says HISTORICAL references
# ("removed in v2.15.0") are legitimate and must not be rewritten — only the two *status*
# shapes it greps for:
#
#     **Status**: v2.14.1 released                 →  `vX.Y.Z released`
#     **FraiseQL** (v2.14.1 released) ships …      →  `(vX.Y.Z released)`
#
# Both reduce to the same token, so one substitution covers them; what matters is that it is
# anchored on the word `released` and not on a bare version shape.
#
# Usage: bump_doc_status_lines <version> <file...>
bump_doc_status_lines() {
    local version="$1"
    shift
    local f
    for f in "$@"; do
        [ -f "$f" ] || continue
        sed -i -E "s/v[0-9]+\.[0-9]+\.[0-9]+ released/v${version} released/g" "$f"
    done
}

# Rewrite README.md's install snippet to the version being released.
#
# Replaces tools/release.sh's step 3, which could not do this and was worse than
# nothing (#1146). That step was:
#
#     if grep -qF "v${VERSION}" "$README"; then  skip  else  sed -i "s/vX.Y.Z/v${VERSION}/g"
#
# Two independent defects, and which one you got was decided by prose elsewhere in the
# file:
#
#   – The guard was satisfied by a HISTORICAL mention. README.md carries the sentence
#     "…v2.15.0 — they had never been exercised against a real database…", about a past
#     removal. `grep -qF "v2.15.0"` matches it, so a 2.15.0 cut printed "badge already at
#     v2.15.0 — skipping" and did nothing. Observed in the #1134 rehearsal.
#   – When it did run, the pattern could not match the snippet anyway (`version = "2.8"`
#     has no `v` prefix and two components), while the blanket `/g` WOULD have rewritten
#     that historical sentence — corrupting the release record.
#
# The README has no version badge at all; the only version-tracking text is the install
# snippet. So this is anchored on what the line MEANS, like bump_deploy_artifacts and
# bump_doc_status_lines, and never on a bare version shape.
#
# Usage: bump_readme_install_snippet <version> <README.md>
bump_readme_install_snippet() {
    local version="$1" readme="$2"
    [ -f "$readme" ] || return 0
    sed -i -E "s|^(fraiseql = \{ *version *= *)\"[^\"]*\"|\1\"${version}\"|" "$readme"
}

# Rotate CHANGELOG.md's Keep a Changelog link definitions at cut time.
#
# The file follows Keep a Changelog — `## [Unreleased]` and `## [x.y.z]` headings — but
# defined none of the link targets the convention specifies, so every version heading
# rendered as a dead link on GitHub and on every release page (#1131). The block is now
# in the file; this keeps it from going stale one release later, which is the state that
# produced #1129, #1134 and #1146.
#
# Idempotent, like every other step in tools/release.sh: if the version already has a
# definition, only the `[Unreleased]` anchor is re-pointed.
#
# Usage: rotate_changelog_links <version> <CHANGELOG.md> [repo_url]
rotate_changelog_links() {
    local version="$1" changelog="$2"
    local repo="${3:-https://github.com/fraiseql/fraiseql}"
    [ -f "$changelog" ] || return 0

    # The current anchor: `[Unreleased]: <repo>/compare/vPREV...HEAD`.
    local prev
    prev="$(sed -nE 's|^\[Unreleased\]: .*/compare/v([^.]+\.[^.]+\.[^.]+)\.\.\.HEAD[[:space:]]*$|\1|p' "$changelog" | head -1)"

    if [ -z "$prev" ]; then
        echo "ERROR: rotate_changelog_links: no '[Unreleased]: …/compare/vX.Y.Z...HEAD' line in $changelog." >&2
        echo "       The link-definition block is missing or malformed. Restore it before cutting," >&2
        echo "       or every version heading ships as a dead link again (#1131)." >&2
        return 1
    fi

    if ! grep -qE "^\[${version//./\.}\]: " "$changelog"; then
        # Insert the new version's line directly after the [Unreleased] anchor.
        sed -i -E "s|^(\[Unreleased\]: .*)$|\1\n[${version}]: ${repo}/compare/v${prev}...v${version}|" "$changelog"
    fi

    # Re-point [Unreleased] at the version just cut.
    sed -i -E "s|^\[Unreleased\]: .*/compare/v[^.]+\.[^.]+\.[^.]+\.\.\.HEAD[[:space:]]*$|[Unreleased]: ${repo}/compare/v${version}...HEAD|" "$changelog"
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

# Fold `## [Unreleased]` into the existing `## [<version>] - <old date>` section and re-date
# it to <date>: what a re-cut of a version that was prepared but never tagged needs. Without
# it, release.sh's "is the header present?" check compared against *today's* date, so a
# re-cut on another day inserted a second, empty `## [<version>]` section above the first.
#
# The version section must be the one directly below `[Unreleased]` (the untagged one):
# a released section is history and is never reached. Each `### <Heading>` of
# `[Unreleased]` merges into the same heading of the version section, its items first
# (newest on top); a heading the version section lacks is added in Keep a Changelog order
# (Breaking, Added, Changed, Deprecated, Removed, Fixed, Security, Known issues, then any
# other). `[Unreleased]` is left empty. Idempotent: with nothing unreleased it only re-dates.
#
# Usage: fold_unreleased_into_version <version> <date> <changelog-file>
fold_unreleased_into_version() {
    local version="$1" date="$2" changelog="$3"
    if ! grep -qE "^## \[${version//./\\.}\] - [0-9]{4}-[0-9]{2}-[0-9]{2}[[:space:]]*$" "$changelog"; then
        echo "ERROR: fold_unreleased_into_version: no '## [${version}] - YYYY-MM-DD' section in $changelog." >&2
        return 1
    fi
    local out
    out="$(mktemp)"
    if ! awk -v ver="$version" -v date="$date" '
        function rank(h) {
            if (h == "Breaking") return 0;   if (h == "Added") return 1
            if (h == "Changed") return 2;    if (h == "Deprecated") return 3
            if (h == "Removed") return 4;    if (h == "Fixed") return 5
            if (h == "Security") return 6;   if (h == "Known issues") return 7
            return 8
        }
        # Split body lines b[1..n] into pre (before the first ###) and per-heading blocks.
        function parse(b, n, tag,    i, h, line) {
            h = ""
            for (i = 1; i <= n; i++) {
                line = b[i]
                if (line ~ /^### /) {
                    h = substr(line, 5)
                    if (!((tag, h) in body)) { body[tag, h] = ""; seen[h] = 1
                        if (!(h in first)) { first[h] = ++nh; hs[nh] = h } }
                    continue
                }
                if (h == "") pre[tag] = pre[tag] line "\n"
                else body[tag, h] = body[tag, h] line "\n"
            }
        }
        function trim(s) { sub(/^\n+/, "", s); sub(/\n+$/, "", s); return s }
        function emit(    i, j, k, h, t, order, u, v, p) {
            print "## [" ver "] - " date
            p = trim(pre["V"]); u = trim(pre["U"])
            if (u != "") p = (p == "" ? u : u "\n\n" p)
            if (p != "") { print ""; print p }
            # Order headings by rank, then by first appearance (version section first).
            for (i = 1; i <= nh; i++) order[i] = hs[i]
            for (i = 2; i <= nh; i++) { t = order[i]
                for (j = i - 1; j >= 1 && (rank(order[j]) > rank(t) || (rank(order[j]) == rank(t) && first[order[j]] > first[t])); j--)
                    order[j + 1] = order[j]
                order[j + 1] = t }
            for (k = 1; k <= nh; k++) {
                h = order[k]; u = trim(body["U", h]); v = trim(body["V", h])
                if (u == "" && v == "") continue
                print ""; print "### " h; print ""
                if (u != "" && v != "") print u "\n\n" v
                else print (u != "" ? u : v)
            }
            print ""
        }
        state == "R" { print; next }
        state == "" && /^## \[Unreleased\]/ { print; print ""; state = "U"; next }
        state == "" { print; next }
        state == "U" && /^## \[/ {
            if (index($0, "## [" ver "] - ") != 1) { bad = 1; exit 3 }
            state = "V"; next
        }
        state == "U" { u[++nu] = $0; next }
        state == "V" && /^## \[/ {
            parse(v, nv, "V"); parse(u, nu, "U"); emit(); print; state = "R"; next
        }
        state == "V" { v[++nv] = $0; next }
        END {
            if (bad) exit 3
            if (state == "V") { parse(v, nv, "V"); parse(u, nu, "U"); emit() }
            else if (state != "R") exit 4
        }
    ' "$changelog" > "$out"; then
        rm -f "$out"
        echo "ERROR: fold_unreleased_into_version: '## [${version}]' is not the section directly below '## [Unreleased]' in $changelog;" >&2
        echo "       only the untagged section is folded into, never a released one." >&2
        return 1
    fi
    mv "$out" "$changelog"
}
